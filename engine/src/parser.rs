use crate::types::{ParsedFile, RuntimeConfig, Settings};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
    sync::Mutex,
};

struct Worker {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

pub struct ParserService {
    runtime: RuntimeConfig,
    worker: Mutex<Option<Worker>>,
    temporary: PathBuf,
    cancelled: AtomicBool,
    cancel: tokio::sync::Notify,
}

impl ParserService {
    pub fn new(runtime: RuntimeConfig, temporary: PathBuf) -> Self {
        Self {
            runtime,
            worker: Mutex::new(None),
            temporary,
            cancelled: AtomicBool::new(false),
            cancel: tokio::sync::Notify::new(),
        }
    }

    async fn spawn(&self) -> Result<Worker> {
        if !self.runtime.parser_runtime.is_file() || !self.runtime.parser_script.is_file() {
            bail!("文件解析运行时缺失，请重新安装应用");
        }
        let mut command = tokio::process::Command::new(&self.runtime.parser_runtime);
        command
            .arg("--max-old-space-size=512")
            .arg(&self.runtime.parser_script)
            .arg("--server")
            .current_dir(&self.temporary)
            .env("TMPDIR", &self.temporary)
            .env("TEMP", &self.temporary)
            .env("TMP", &self.temporary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        if self.runtime.electron_runtime {
            command.env("ELECTRON_RUN_AS_NODE", "1");
        }
        #[cfg(windows)]
        command.creation_flags(0x08004000);
        let mut child = command.spawn().context("无法启动文件解析程序")?;
        crate::processes::track(child.id().context("解析进程编号不可用")?)?;
        let input = child.stdin.take().context("解析器输入不可用")?;
        let output = BufReader::new(child.stdout.take().context("解析器输出不可用")?);
        Ok(Worker {
            child,
            input,
            output,
        })
    }

    pub async fn parse(&self, path: &Path, settings: &Settings) -> Result<ParsedFile> {
        if self.cancelled.load(Ordering::SeqCst) {
            bail!("文件解析已停止");
        }
        let mut worker = self.worker.lock().await;
        let alive = worker
            .as_mut()
            .is_some_and(|worker| worker.child.try_wait().is_ok_and(|status| status.is_none()));
        if !alive {
            *worker = Some(self.spawn().await?);
        }
        let active = worker.as_mut().unwrap();
        let input=json!({"path":path,"resources":self.runtime.resources,"ocr_enabled":settings.ocr_enabled}).to_string()+"\n";
        let parsing = tokio::time::timeout(Duration::from_secs(600), async {
            active.input.write_all(input.as_bytes()).await?;
            active.input.flush().await?;
            let mut line = String::new();
            if active.output.read_line(&mut line).await? == 0 {
                bail!("文件解析程序已退出，将自动重新启动");
            }
            if line.len() > 128 * 1024 * 1024 {
                bail!("解析结果过大，请拆分文档");
            }
            let result: Value =
                serde_json::from_str(&line).context("文件解析程序未返回有效结果")?;
            Ok::<_, anyhow::Error>(result)
        });
        if self.cancelled.load(Ordering::SeqCst) {
            bail!("文件解析已停止");
        }
        let response = tokio::select! {
            response=parsing=>response,
            _=self.cancel.notified()=>{
                if let Some(mut old)=worker.take(){let _=old.child.kill().await;}
                bail!("文件解析已取消");
            }
        };
        let value = match response {
            Ok(Ok(value)) => value,
            result => {
                if let Some(mut old) = worker.take() {
                    let _ = old.child.kill().await;
                }
                return match result {
                    Ok(Err(error)) => Err(error),
                    _ => Err(anyhow::anyhow!("文件解析超时，可拆分文档后重试")),
                };
            }
        };
        if value["ok"] != true {
            bail!("{}", value["error"].as_str().unwrap_or("文件解析失败"));
        }
        Ok(serde_json::from_value(value)?)
    }

    pub async fn stop(&self) {
        self.request_stop();
        if let Some(mut worker) = self.worker.lock().await.take() {
            let _ = worker.child.kill().await;
        }
    }
    pub fn request_stop(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.cancel.notify_waiters();
    }
}

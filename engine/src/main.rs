mod api;
mod backup;
mod connections;
mod indexer;
mod knowledge;
mod mcp;
mod models;
mod parser;
mod retrieval;
mod state;
mod store;
#[cfg(test)]
mod tests;
mod types;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use fs2::FileExt;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
use tower_http::services::{ServeDir, ServeFile};
use types::*;

#[derive(Parser)]
#[command(name = "knowledge-engine", version, about = "本地个人知识服务")]
struct Arguments {
    #[arg(long, global = true)]
    home: Option<PathBuf>,
    #[arg(long, global = true)]
    runtime: Option<PathBuf>,
    #[command(subcommand)]
    command: Mode,
}

#[derive(Subcommand)]
enum Mode {
    Serve,
    Mcp {
        #[arg(long)]
        connection: String,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("warn")
        .init();
    if let Err(error) = run().await {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let arguments = Arguments::parse();
    let home = arguments.home.unwrap_or_else(default_home);
    std::fs::create_dir_all(&home)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700))?;
    }
    let runtime_path = arguments
        .runtime
        .unwrap_or_else(|| home.join("runtime.json"));
    let runtime: RuntimeConfig = std::fs::read(runtime_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    match arguments.command {
        Mode::Serve => serve(home, runtime).await,
        Mode::Mcp { connection } => mcp::stdio(&home, connection, runtime).await,
    }
}

fn default_home() -> PathBuf {
    directories::ProjectDirs::from("local", "personal", "Personal Knowledge")
        .map(|directories| directories.data_local_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".personal-knowledge"))
}

async fn serve(home: PathBuf, runtime: RuntimeConfig) -> Result<()> {
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(home.join("service.lock"))?;
    if lock.try_lock_exclusive().is_err() {
        return Ok(());
    }
    let app = state::AppState::new(home.clone(), runtime.clone())?;
    let preferred = app.store.metadata::<u16>("http_port")?.unwrap_or(17481);
    let listener =
        match tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, preferred)).await {
            Ok(listener) => listener,
            Err(_) => tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?,
        };
    let port = listener.local_addr()?.port();
    app.store.set_metadata("http_port", &port)?;
    let token = format!(
        "{}{}",
        identifier().replace('-', ""),
        identifier().replace('-', "")
    );
    let info = ServiceInfo {
        port,
        pid: std::process::id(),
        token: token.clone(),
        version: env!("CARGO_PKG_VERSION").into(),
    };
    connections::private_write(&home.join("service.json"), &serde_json::to_vec(&info)?)?;
    let mut router = api::router(app.clone(), token);
    if runtime.web_root.is_dir() {
        router = router.fallback_service(
            ServeDir::new(&runtime.web_root)
                .fallback(ServeFile::new(runtime.web_root.join("index.html"))),
        );
    }
    let worker = app.clone();
    tokio::spawn(async move {
        indexer::run(worker).await;
    });
    let stopping = app.clone();
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            loop {
                tokio::select! {
                    _=tokio::signal::ctrl_c()=>break,
                    _=tokio::time::sleep(Duration::from_millis(200))=>{
                        if stopping.stopping.load(std::sync::atomic::Ordering::SeqCst){break;}
                    },
                }
            }
        })
        .await?;
    app.stop();
    app.models.stop().await;
    app.parser.stop().await;
    let _ = std::fs::remove_file(home.join("service.json"));
    let _ = lock.unlock();
    Ok(())
}

pub async fn ensure_service(home: &Path, runtime: &RuntimeConfig) -> Result<ServiceInfo> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    if let Some(info) = read_service(home) {
        if healthy(&client, &info).await {
            return Ok(info);
        }
    }
    if !runtime.parser_runtime.as_os_str().is_empty() {
        connections::private_write(&home.join("runtime.json"), &serde_json::to_vec(runtime)?)?;
    }
    let executable = std::env::current_exe()?;
    let mut command = Command::new(executable);
    command
        .arg("serve")
        .arg("--home")
        .arg(home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000008);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .spawn()
        .context("无法启动本地知识服务，请打开知识库窗口查看安装状态")?;
    for _ in 0..80 {
        tokio::time::sleep(Duration::from_millis(150)).await;
        if let Some(info) = read_service(home) {
            if healthy(&client, &info).await {
                return Ok(info);
            }
        }
    }
    bail!("知识服务启动超时，请重新打开应用")
}

fn read_service(home: &Path) -> Option<ServiceInfo> {
    std::fs::read(home.join("service.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}
async fn healthy(client: &reqwest::Client, info: &ServiceInfo) -> bool {
    client
        .get(format!("http://127.0.0.1:{}/api/health", info.port))
        .bearer_auth(&info.token)
        .send()
        .await
        .is_ok_and(|response| response.status().is_success())
}

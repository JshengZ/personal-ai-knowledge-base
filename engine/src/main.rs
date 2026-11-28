mod api;
mod errors;
mod diagnostics;
mod queries;
mod appearance;
mod backup;
mod connections;
mod counters;
mod indexer;
mod knowledge;
mod logging;
mod mcp;
mod models;
mod network;
mod onboarding;
mod parser;
mod processes;
mod progress;
mod rules;
mod collections;
mod retrieval;
mod state;
mod storage;
mod store;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod collection_tests;
#[cfg(test)]
mod review_regressions;
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
    #[arg(long,global=true,value_parser=["service","mcp-stdio","storage-migration"])]
    role:Option<String>,
    #[command(subcommand)]
    command: Mode,
}

#[derive(Subcommand)]
enum Mode {
    Serve,
    MigrateData {
        #[arg(long)]
        destination: PathBuf,
    },
    Mcp {
        #[arg(long)]
        connection: String,
    },
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        tracing::error!(error=%format!("{error:#}"),"knowledge service failed");
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let arguments = Arguments::parse();
    let role=match &arguments.command {Mode::Serve=>"service",Mode::Mcp{..}=>"mcp-stdio",Mode::MigrateData{..}=>"storage-migration"};
    if arguments.role.as_deref().is_some_and(|value|value!=role) {bail!("进程 role 与子命令不匹配");}
    let home = storage::resolve_home(&arguments.home.unwrap_or_else(default_home))?;
    if let Mode::MigrateData { destination } = &arguments.command {
        println!("{}", storage::migrate(&home, destination)?);
        return Ok(());
    }
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
        Mode::MigrateData { .. } => unreachable!(),
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
    logging::initialize(&home)?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        pid = std::process::id(),
        "knowledge service starting"
    );
    let app = state::AppState::new(home.clone(), runtime.clone())?;
    let preferred = app.store.metadata::<u16>("http_port")?.unwrap_or(17481);
    let listener =
        match tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, preferred)).await {
            Ok(listener) => listener,
            Err(_) => tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?,
        };
    let port = listener.local_addr()?.port();
    tracing::info!(port, "loopback HTTP and MCP listener ready");
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
        runtime: runtime.clone(),
    };
    connections::private_write(&home.join("service.json"), &serde_json::to_vec(&info)?)?;
    if let Err(error) = storage::finish_migration(&home) {
        let _ = app.store.event(
            "storage",
            "迁移已完成，旧位置部分文件仍待清理",
            serde_json::json!({"error":error.to_string()}),
        );
    }
    let mut router = api::router(app.clone(), token, port);
    if runtime.web_root.is_dir() {
        router = router.fallback_service(
            ServeDir::new(&runtime.web_root)
                .fallback(ServeFile::new(runtime.web_root.join("index.html"))),
        );
    }
    let worker = app.clone();
    let counting = app.clone();
    let counters=tokio::task::spawn_blocking(move || {
        if let Err(error) = crate::counters::backfill(&counting.store) {
            tracing::warn!(%error,"progress counter initialization interrupted");
        }
        counting.wake.notify_one();
    });
    let runtime_handle = tokio::runtime::Handle::current();
    let indexing=tokio::task::spawn_blocking(move || runtime_handle.block_on(indexer::run(worker)));
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
    for result in [indexing.await,counters.await] {if let Err(error)=result {tracing::error!(%error,"service worker join failed");}}
    app.models.stop().await;
    app.parser.stop().await;
    let _ = std::fs::remove_file(home.join("service.json"));
    let _ = lock.unlock();
    tracing::info!("knowledge service stopped; parser and model workers released");
    Ok(())
}

pub async fn ensure_service(home: &Path, runtime: &RuntimeConfig) -> Result<ServiceInfo> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()?;
    let start=std::time::Instant::now();
    let startup=std::fs::OpenOptions::new().read(true).write(true).create(true).open(home.join("startup.lock"))?;
    loop {
        if let Some(info)=read_service(home) {
            if healthy(&client,&info).await {
                if info.version!=env!("CARGO_PKG_VERSION") {return Err(errors::ServiceError::new("service_version_mismatch",format!("后台服务版本 {} 与当前引擎 {} 不一致，请退出旧服务后重新启动",info.version,env!("CARGO_PKG_VERSION")),axum::http::StatusCode::CONFLICT).into());}
                return Ok(info);
            }
        }
        if startup.try_lock_exclusive().is_ok() {break;}
        if start.elapsed()>Duration::from_secs(25) {return Err(errors::ServiceError::new("service_starting","另一个进程正在启动共享后台服务，请稍后重试",axum::http::StatusCode::SERVICE_UNAVAILABLE).into());}
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    // Recheck after acquiring the inter-process startup lock.
    if let Some(info)=read_service(home) {if healthy(&client,&info).await {
        if info.version!=env!("CARGO_PKG_VERSION") {return Err(errors::ServiceError::new("service_version_mismatch",format!("后台服务版本 {} 与当前引擎 {} 不一致，请退出旧服务后重新启动",info.version,env!("CARGO_PKG_VERSION")),axum::http::StatusCode::CONFLICT).into());}
        return Ok(info);
    }}

    if !runtime.parser_runtime.as_os_str().is_empty() {
        connections::private_write(&home.join("runtime.json"), &serde_json::to_vec(runtime)?)?;
    }
    let executable = std::env::current_exe()?;
    let temporary = home.join("tmp");
    std::fs::create_dir_all(&temporary)?;
    let mut command = Command::new(executable);
    command
        .arg("serve")
        .args(["--role","service"])
        .arg("--home")
        .arg(home)
        .current_dir(&temporary)
        .env("TMPDIR", &temporary)
        .env("TEMP", &temporary)
        .env("TMP", &temporary)
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
        .context("无法启动独立后台知识服务，请检查安装路径及本机服务日志")?;
    for _ in 0..80 {
        tokio::time::sleep(Duration::from_millis(150)).await;
        if let Some(info) = read_service(home) {
            if healthy(&client, &info).await {
                return Ok(info);
            }
        }
    }
    Err(errors::ServiceError::new("service_start_timeout","独立后台知识服务启动超时，请检查 service.log",axum::http::StatusCode::SERVICE_UNAVAILABLE).into())
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

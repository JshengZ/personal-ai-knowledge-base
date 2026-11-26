use anyhow::Result;
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};

struct State {
    path: PathBuf,
    file: Option<File>,
    bytes: u64,
}
#[derive(Clone)]
struct Rolling(Arc<Mutex<State>>);
struct Writer<'a>(MutexGuard<'a, State>);

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Rolling {
    type Writer = Writer<'a>;
    fn make_writer(&'a self) -> Self::Writer {
        Writer(self.0.lock().unwrap())
    }
}
impl Write for Writer<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let state = &mut self.0;
        if state.bytes + bytes.len() as u64 > 4 * 1024 * 1024 {
            state.file.take();
            let rotated = state.path.with_extension("log.1");
            if rotated.exists() {
                std::fs::remove_file(&rotated)?;
            }
            if state.path.exists() {
                std::fs::rename(&state.path, rotated)?;
            }
            state.file = Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&state.path)?,
            );
            state.bytes = 0;
        }
        if state.file.is_none() {
            state.file = Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&state.path)?,
            );
        }
        let written = state.file.as_mut().unwrap().write(bytes)?;
        state.bytes += written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = self.0.file.as_mut() {
            file.flush()
        } else {
            Ok(())
        }
    }
}

pub fn initialize(home: &std::path::Path) -> Result<()> {
    let path = home.join("service.log");
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    let bytes = file.metadata()?.len();
    let rolling = Rolling(Arc::new(Mutex::new(State {
        path,
        file: Some(file),
        bytes,
    })));
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter("info")
        .with_writer(rolling)
        .try_init()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        pid = std::process::id(),
        "knowledge service starting"
    );
    Ok(())
}

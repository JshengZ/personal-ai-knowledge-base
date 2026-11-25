use crate::{
    models::Models,
    state::{modified_ms, AppState},
    types::*,
};
use anyhow::{bail, Context, Result};
use globset::{Glob, GlobSetBuilder};
use notify::{RecursiveMode, Watcher};
use serde_json::json;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tokio::sync::mpsc;
use walkdir::WalkDir;

pub fn supported(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();
    matches!(
        extension.as_str(),
        "md" | "mdx"
            | "txt"
            | "rst"
            | "log"
            | "json"
            | "jsonl"
            | "yaml"
            | "yml"
            | "toml"
            | "ini"
            | "cfg"
            | "py"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "rs"
            | "go"
            | "java"
            | "c"
            | "h"
            | "cpp"
            | "hpp"
            | "cs"
            | "swift"
            | "kt"
            | "scala"
            | "sql"
            | "sh"
            | "bash"
            | "zsh"
            | "ps1"
            | "r"
            | "R"
            | "m"
            | "html"
            | "htm"
            | "css"
            | "scss"
            | "vue"
            | "svelte"
            | "ipynb"
            | "pdf"
            | "docx"
            | "pptx"
            | "xlsx"
            | "csv"
            | "tsv"
            | "png"
            | "jpg"
            | "jpeg"
            | "tif"
            | "tiff"
    )
}

pub fn validate_exclusions(patterns: &[String]) -> Result<()> {
    for pattern in patterns {
        Glob::new(pattern).context("排除规则格式无效")?;
    }
    Ok(())
}

fn eligible(path: &Path, root: &Root, state: &AppState) -> Result<bool> {
    if path.starts_with(&state.home) {
        return Ok(false);
    }
    let Ok(relative) = path.strip_prefix(&root.path) else {
        return Ok(false);
    };
    if relative.components().any(|part| {
        matches!(
            part.as_os_str().to_str(),
            Some(
                ".git"
                    | "node_modules"
                    | ".venv"
                    | "venv"
                    | "target"
                    | ".next"
                    | ".cache"
                    | "__pycache__"
                    | "$RECYCLE.BIN"
            )
        )
    }) {
        return Ok(false);
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in &root.exclusions {
        builder.add(Glob::new(pattern)?);
    }
    Ok(!builder.build()?.is_match(relative) && supported(path))
}

pub async fn scan_all(state: Arc<AppState>, force: bool) -> Result<()> {
    let _maintenance = state.maintenance.read().await;
    if state.scanning.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let copy = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        for mut root in copy.store.roots()? {
            if !root.enabled {
                continue;
            }
            match scan_root(&copy, &root, force) {
                Ok(()) => {
                    root.error = None;
                    root.last_scan = Some(timestamp());
                }
                Err(error) => {
                    root.error = Some(error.to_string());
                }
            }
            copy.store.save_root(&root)?;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await;
    state.scanning.store(false, Ordering::SeqCst);
    result??;
    state.store.set_metadata("last_scan", &timestamp())?;
    Ok(())
}

fn scan_root(state: &AppState, root: &Root, force: bool) -> Result<()> {
    let directory = Path::new(&root.path)
        .canonicalize()
        .context("目录不可用，请重新关联目录")?;
    let mut builder = GlobSetBuilder::new();
    for pattern in &root.exclusions {
        builder.add(Glob::new(pattern)?);
    }
    let excludes = builder.build()?;
    let mut found = HashSet::new();
    let mut walk_error = false;
    for item in WalkDir::new(&directory)
        .follow_links(false)
        .max_depth(100)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            if entry.path().starts_with(&state.home) {
                return false;
            }
            let name = entry.file_name().to_string_lossy();
            if entry.file_type().is_dir()
                && matches!(
                    name.as_ref(),
                    ".git"
                        | "node_modules"
                        | ".venv"
                        | "venv"
                        | "target"
                        | ".next"
                        | ".cache"
                        | "__pycache__"
                        | "$RECYCLE.BIN"
                )
            {
                return false;
            }
            let relative = entry
                .path()
                .strip_prefix(&directory)
                .unwrap_or(entry.path());
            !excludes.is_match(relative)
        })
    {
        let entry = match item {
            Ok(entry) => entry,
            Err(_) => {
                walk_error = true;
                continue;
            }
        };
        if !entry.file_type().is_file() || !supported(entry.path()) {
            continue;
        }
        let path = entry.path().to_string_lossy().into_owned();
        found.insert(path.clone());
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => {
                walk_error = true;
                continue;
            }
        };
        let changed = state.store.document_by_path(&path)?.is_none_or(|document| {
            document.size != metadata.len()
                || document.modified_ms != modified_ms(&metadata)
                || document.status == "indexing"
        });
        if force || changed {
            state.store.enqueue(&path, &root.id, force)?;
        }
    }
    if !walk_error {
        for document in state
            .store
            .documents()?
            .into_iter()
            .filter(|document| document.root_id == root.id)
        {
            if !found.contains(&document.path) {
                state.store.remove_document(&document.id)?;
            }
        }
    } else {
        bail!("部分目录无法访问，已保留未能核对的资料记录");
    }
    Ok(())
}

pub async fn run(state: Arc<AppState>) {
    let (sender, mut changes) = mpsc::channel::<Vec<PathBuf>>(256);
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event {
            let _ = sender.try_send(event.paths);
        }
    })
    .ok();
    let mut watching = HashSet::<String>::new();
    let _ = scan_all(state.clone(), false).await;
    let worker = state.clone();
    tokio::spawn(async move {
        run_jobs(worker).await;
    });
    let worker = state.clone();
    tokio::spawn(async move {
        run_embeddings(worker).await;
    });
    let worker = state.clone();
    tokio::spawn(async move {
        crate::knowledge::run_summaries(worker).await;
    });
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    let mut last_scan = std::time::Instant::now();
    while !state.stopping.load(Ordering::SeqCst) {
        if let Ok(roots) = state.store.roots() {
            let enabled: HashSet<_> = roots
                .into_iter()
                .filter(|root| root.enabled)
                .map(|root| root.path)
                .collect();
            if let Some(watcher) = watcher.as_mut() {
                for old in watching.difference(&enabled) {
                    let _ = watcher.unwatch(Path::new(old));
                }
                for new in enabled.difference(&watching) {
                    if let Err(error) = watcher.watch(Path::new(new), RecursiveMode::Recursive) {
                        let _ = state.store.event(
                            "watcher",
                            "文件监听暂不可用，定时核对仍在运行",
                            json!({"reason":error.to_string()}),
                        );
                    }
                }
            }
            watching = enabled;
        }
        tokio::select! {
            event=changes.recv()=>{
                if let Some(mut paths)=event {
                    tokio::time::sleep(Duration::from_millis(900)).await;
                    while let Ok(extra)=changes.try_recv() {paths.extend(extra);}
                    if !state.store.settings().is_ok_and(|settings|settings.paused) {
                        let roots=state.store.roots().unwrap_or_default();
                        for path in paths {
                            if let Some(root)=roots.iter().filter(|root|root.enabled && path.starts_with(&root.path)).max_by_key(|root|root.path.len()) {
                                if path.is_file() && supported(&path) {let _=state.store.enqueue(&path.to_string_lossy(),&root.id,true);}
                            }
                        }
                        let _=scan_all(state.clone(),false).await;
                        last_scan=std::time::Instant::now();
                    }
                }
            },
            _=state.wake.notified()=>{let _=scan_all(state.clone(),false).await;},
            _=interval.tick()=>{
                let settings=state.store.settings().unwrap_or_default();
                if !settings.paused && last_scan.elapsed().as_secs()>=settings.reconcile_seconds.clamp(30,3600) {
                    let _=scan_all(state.clone(),false).await;
                    last_scan=std::time::Instant::now();
                }
            },
        }
    }
}

async fn run_jobs(state: Arc<AppState>) {
    while !state.stopping.load(Ordering::SeqCst) {
        if state.store.settings().is_ok_and(|settings| settings.paused) {
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        }
        let job = state.store.next_job().unwrap_or(None);
        if let Some((path, root_id, force, attempts)) = job {
            let _maintenance = state.maintenance.read().await;
            match index_file(&state, &path, &root_id, force).await {
                Ok(()) => {
                    let _ = state.store.finish_job(&path);
                }
                Err(error) => {
                    let message = error.to_string();
                    if let Ok(Some(mut document)) = state.store.document_by_path(&path) {
                        document.status = "error".into();
                        document.error = Some(message.clone());
                        let _ = state.store.save_document(&document);
                    }
                    let _ = state.store.event(
                        "index_error",
                        "资料索引需要重试",
                        json!({"path":path,"error":message,"attempt":attempts+1}),
                    );
                    if attempts >= 5 {
                        let _ = state.store.finish_job(&path);
                    } else {
                        let _ = state.store.retry_job(&path, attempts);
                    }
                }
            }
        } else {
            tokio::time::sleep(Duration::from_millis(350)).await;
        }
    }
}

async fn index_file(state: &AppState, path: &str, root_id: &str, force: bool) -> Result<()> {
    let root = state.store.root(root_id)?;
    if !root.enabled {
        return Ok(());
    }
    if !eligible(Path::new(path), &root, state)? {
        return Ok(());
    }
    let canonical = match Path::new(path).canonicalize() {
        Ok(path) => path,
        Err(_) => {
            if let Some(document) = state.store.document_by_path(path)? {
                state.store.remove_document(&document.id)?;
            }
            return Ok(());
        }
    };
    if !canonical.starts_with(Path::new(&root.path).canonicalize()?) || !canonical.is_file() {
        return Ok(());
    }
    let metadata = tokio::fs::metadata(&canonical).await?;
    let settings = state.store.settings()?;
    let previous = state.store.document_by_path(path)?;
    if !force
        && previous.as_ref().is_some_and(|document| {
            document.status == "ready"
                && document.modified_ms == modified_ms(&metadata)
                && document.size == metadata.len()
        })
    {
        return Ok(());
    }
    let project_path = find_project(&canonical, Path::new(&root.path));
    let project_name = project_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(&root.name);
    let mut project = state
        .store
        .ensure_project(&project_path.to_string_lossy(), project_name)?;
    let mut document = Document {
        id: previous
            .as_ref()
            .map(|value| value.id.clone())
            .unwrap_or_else(identifier),
        path: path.into(),
        title: canonical
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        root_id: root_id.into(),
        project_id: project.id.clone(),
        extension: canonical
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase(),
        version: String::new(),
        size: metadata.len(),
        modified_ms: modified_ms(&metadata),
        updated_at: timestamp(),
        status: "indexing".into(),
        error: None,
        chunks: 0,
        ocr: false,
    };
    if let Some(previous) = &previous {
        document.version = previous.version.clone();
    }
    state.store.save_document(&document)?;
    if metadata.len() > settings.max_file_mb.clamp(1, 100) * 1024 * 1024 {
        bail!("文件超过当前大小限制，可拆分后加入");
    }
    let bytes = tokio::fs::read(&canonical).await?;
    let version = blake3::hash(&bytes).to_hex().to_string();
    if previous
        .as_ref()
        .is_some_and(|value| value.version == version && value.status == "ready")
        && !force
    {
        document.status = "ready".into();
        document.chunks = previous.unwrap().chunks;
        state.store.save_document(&document)?;
        return Ok(());
    }
    let parsed = parse(state, &canonical, &settings).await?;
    let after = tokio::fs::read(&canonical).await?;
    if blake3::hash(&after).to_hex().as_str() != version {
        bail!("文件仍在保存，将在写入稳定后重试");
    }
    let after_metadata = tokio::fs::metadata(&canonical).await?;
    let current_root = state.store.root(root_id)?;
    if !current_root.enabled {
        return Ok(());
    }
    document.version = version;
    document.modified_ms = modified_ms(&after_metadata);
    document.size = after_metadata.len();
    document.ocr = parsed.ocr;
    let chunks: Vec<Chunk> = parsed
        .blocks
        .into_iter()
        .filter(|block| !block.text.trim().is_empty())
        .enumerate()
        .map(|(ordinal, block)| {
            let hash = blake3::hash(
                format!("{}:{}:{}", document.id, document.version, ordinal).as_bytes(),
            )
            .to_hex()
            .to_string();
            Chunk {
                id: hash,
                document_id: document.id.clone(),
                ordinal,
                text: block.text,
                location: block.location,
            }
        })
        .collect();
    document.chunks = chunks.len();
    document.status = if chunks.is_empty() { "empty" } else { "ready" }.into();
    state.store.replace_document(&document, &chunks)?;
    project.updated_at = timestamp();
    state.store.save_project(&project)?;
    state.store.set_metadata("last_indexed", &timestamp())?;
    Ok(())
}

fn find_project(path: &Path, root: &Path) -> PathBuf {
    let mut directory = path.parent();
    while let Some(current) = directory {
        if !current.starts_with(root) {
            break;
        }
        if current.join(".git").exists() {
            return current.into();
        }
        if current == root {
            break;
        }
        directory = current.parent();
    }
    root.into()
}

async fn parse(state: &AppState, path: &Path, settings: &Settings) -> Result<ParsedFile> {
    state.parser.parse(path, settings).await
}

async fn run_embeddings(state: Arc<AppState>) {
    while !state.stopping.load(Ordering::SeqCst) {
        let settings = state.store.settings().unwrap_or_default();
        if settings.paused || !state.models.embedding_available(&settings) {
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }
        let _maintenance = state.maintenance.read().await;
        let model = state.models.embedding_model(&settings);
        let chunks = state
            .store
            .pending_embeddings(&model, 8, Models::remote_embeddings(&settings))
            .unwrap_or_default();
        if chunks.is_empty() {
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }
        state.embedding_busy.store(true, Ordering::SeqCst);
        let texts = chunks
            .iter()
            .map(|chunk| chunk.text.clone())
            .collect::<Vec<_>>();
        match state.models.embed(&settings, &texts, false).await {
            Ok(vectors) => {
                let current = state.store.settings().unwrap_or_default();
                if state.models.embedding_model(&current) == model {
                    for (chunk, vector) in chunks.iter().zip(vectors) {
                        if let Ok(document) = state.store.document(&chunk.document_id) {
                            if state
                                .check_document(
                                    &Scope::Owner,
                                    &document,
                                    Models::remote_embeddings(&settings),
                                )
                                .is_ok()
                            {
                                let _ = state.store.save_embedding(&chunk.id, &model, &vector);
                            }
                        }
                    }
                }
                *state.semantic_error.write().await = None;
            }
            Err(error) => {
                *state.semantic_error.write().await = Some(error.to_string());
                tokio::time::sleep(Duration::from_secs(15)).await;
            }
        }
        state.embedding_busy.store(false, Ordering::SeqCst);
    }
}

use crate::{
    models::Models,
    state::{modified_ms, AppState},
    types::*,
};
use anyhow::{bail, Context, Result};
use globset::Glob;
use notify::{EventKind, RecursiveMode, Watcher};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
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

pub(crate) fn eligible(path: &Path, root: &Root, state: &AppState) -> Result<bool> {
    let rules=crate::rules::Rules::new(root,&state.home,&state.runtime.resources)?;
    if rules.skip(path,false).is_some() { return Ok(false); }
    if let Ok(canonical)=path.canonicalize() {
        let mut resolved=root.clone();
        resolved.path=Path::new(&root.path).canonicalize()?.to_string_lossy().into_owned();
        let home=state.home.canonicalize().unwrap_or_else(|_|state.home.clone());
        let resources=state.runtime.resources.canonicalize().unwrap_or_else(|_|state.runtime.resources.clone());
        if crate::rules::Rules::new(&resolved,&home,&resources)?.skip(&canonical,false).is_some() { return Ok(false); }
    }
    Ok(true)
}

pub async fn scan_all(state: Arc<AppState>, force: bool) -> Result<()> {
    if !state.reconciliation_allowed() {
        return Ok(());
    }
    let _maintenance = state.maintenance.read().await;
    if state.scanning.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let revision = state.store.metadata::<u64>("library_revision")?.unwrap_or(0);
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
            // A concurrent edit/removal must never be overwritten by an old scan snapshot.
            let _library=copy.library_lock.lock().unwrap();
            if let Ok(mut current) = copy.store.root(&root.id) {
                if current.path==root.path {
                    current.last_scan=root.last_scan; current.error=root.error;
                    copy.store.save_root(&current)?;
                }
            }
        }
        Ok::<_, anyhow::Error>(())
    })
    .await;
    state.scanning.store(false, Ordering::SeqCst);
    result??;
    state.store.set_metadata("last_scan", &timestamp())?;
    if revision!=state.store.metadata::<u64>("library_revision")?.unwrap_or(0) { state.wake.notify_one(); }
    Ok(())
}

fn scan_root(state: &AppState, root: &Root, force: bool) -> Result<()> {
    let _task=state.diagnostics.task("scan",json!({"root_id":root.id,"name":root.name}));
    reconcile_root(state, root, force, || state.indexing_allowed())
}

pub(crate) fn reconcile_root(
    state: &AppState,
    root: &Root,
    force: bool,
    allow_growth: impl Fn() -> bool,
) -> Result<()> {
    let directory = Path::new(&root.path)
        .canonicalize()
        .context("目录不可用，请重新关联目录")?;
    let rules = crate::rules::Rules::new(root,&state.home,&state.runtime.resources)?;
    let summary = std::cell::RefCell::new(crate::rules::ScanSummary::default());
    let mut found = HashSet::new();
    let mut formats = HashMap::<String, u64>::new();
    let mut walk_error = false;
    let mut deferred = false;
    let mut queued = state.store.pending_jobs()?;
    for item in WalkDir::new(&directory)
        .follow_links(false)
        .max_depth(100)
        .into_iter()
        .filter_entry(|entry| {
            if !entry.file_type().is_dir() { return true; }
            if let Some(reason)=rules.skip(entry.path(),true) {
                summary.borrow_mut().record(root,entry.path(),true,reason);
                false
            } else { true }
        })
    {
        if state.stopping.load(Ordering::SeqCst) {
            return Ok(());
        }
        let entry = match item {
            Ok(entry) => entry,
            Err(_) => {
                walk_error = true;
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        if let Some(reason)=rules.skip(entry.path(),false) {
            let extension=entry.path().extension().and_then(|value|value.to_str()).unwrap_or_default().to_ascii_lowercase();
            if ["ppt","doc","xls","xlsb","docm","pptm","xlsm","pps","ppsx"].contains(&extension.as_str()) { *formats.entry(extension).or_default()+=1; }
            summary.borrow_mut().record(root,entry.path(),false,reason);
            continue;
        }
        summary.borrow_mut().eligible_files+=1;
        let path = entry.path().to_string_lossy().into_owned();
        found.insert(path.clone());
        if found.len() > 100_000 {
            bail!("目录超过 10 万份可索引文件，请缩小资料范围后重试；已有索引已保留");
        }
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
                || matches!(document.status.as_str(), "indexing" | "removing")
        });
        if force || changed {
            if queued < 5000 && allow_growth() {
                queue_file(state,root,Path::new(&path),force,false)?;
                queued += 1;
            } else {
                deferred = true;
            }
        }
    }
    let mut summary=summary.into_inner(); summary.scanned_at=timestamp();
    state.store.set_metadata(&format!("scan_summary:{}",root.id),&summary)?;
    state
        .store
        .set_metadata(&format!("scan_found:{}", root.id), &found.len())?;
    state
        .store
        .set_metadata(&format!("scan_formats:{}", root.id), &formats)?;
    if !walk_error {
        // Cancel obsolete parse jobs without processing them. Reclamation has
        // its own bounded queue and does not need another full filesystem scan
        // for every 64 legacy chunks removed.
        for path in state.store.queued_paths(&root.id)? {
            if !found.contains(&path) && (!Path::new(&path).is_file() || !state.store.root(&root.id).is_ok_and(|latest|eligible(Path::new(&path),&latest,state).unwrap_or(false))) {
                state.store.finish_root_job(&path, &root.id)?;
            }
        }
        let removals = state
            .store
            .documents()?
            .into_iter()
            .filter(|document| document.root_id == root.id)
            .filter(|document| !found.contains(&document.path) && (!Path::new(&document.path).is_file() || !eligible(Path::new(&document.path),root,state).unwrap_or(false)))
            .map(|document| document.id)
            .collect::<Vec<_>>();
        state.store.schedule_cleanup_many(&removals)?;
    } else {
        bail!("部分目录无法访问，已保留未能核对的资料记录");
    }
    state
        .store
        .set_metadata(&format!("scan_deferred:{}", root.id), &deferred)?;
    Ok(())
}

pub async fn run(state: Arc<AppState>) {
    let (sender,mut changes)=mpsc::channel::<Vec<PathBuf>>(256);
    let observer=state.clone();
    let mut watcher=notify::recommended_watcher(move|event:notify::Result<notify::Event>| {
        match event {
            Ok(event) if matches!(event.kind,EventKind::Create(_)|EventKind::Modify(_)|EventKind::Remove(_))=>{
                if sender.try_send(event.paths).is_err() {observer.diagnostics.watcher_drops.fetch_add(1,Ordering::Relaxed);}
            },
            Err(error)=>{observer.diagnostics.watcher_drops.fetch_add(1,Ordering::Relaxed);tracing::warn!(%error,"filesystem watcher requires reconciliation");},
            _=>{},
        }
    }).map_err(|error|tracing::warn!(%error,"filesystem watcher unavailable")).ok();
    let handle=tokio::runtime::Handle::current();
    let worker=state.clone();let runtime=handle.clone();let jobs=tokio::task::spawn_blocking(move||runtime.block_on(run_jobs(worker)));
    let worker=state.clone();let runtime=handle.clone();let cleanup=tokio::task::spawn_blocking(move||runtime.block_on(run_cleanup(worker)));
    let worker=state.clone();let runtime=handle.clone();let embeddings=tokio::task::spawn_blocking(move||runtime.block_on(run_embeddings(worker)));
    let worker=state.clone();let summaries=tokio::task::spawn_blocking(move||handle.block_on(crate::knowledge::run_summaries(worker)));
    let mut watching=HashSet::<String>::new();let mut fingerprints=HashMap::<String,String>::new();
    let mut first=true;let mut paused=state.store.settings().unwrap_or_default().paused;
    let mut last_scan=std::time::Instant::now();let mut seen_drops=0;
    let mut interval=tokio::time::interval(Duration::from_secs(30));
    while !state.stopping.load(Ordering::SeqCst) {
        let roots=state.store.roots().unwrap_or_default();
        let settings=state.store.settings().unwrap_or_default();
        let enabled=roots.iter().filter(|root|root.enabled).map(|root|root.path.clone()).collect::<HashSet<_>>();
        if let Some(watcher)=watcher.as_mut() {
            for path in watching.difference(&enabled).cloned().collect::<Vec<_>>() {let _=watcher.unwatch(Path::new(&path));watching.remove(&path);}
            for root in roots.iter().filter(|root|root.enabled) {
                if watching.contains(&root.path) {continue;}
                let result=watcher.watch(Path::new(&root.path),RecursiveMode::Recursive);
                state.diagnostics.watchers.lock().unwrap().insert(root.id.clone(),json!({"active":result.is_ok(),"backend":"native","error":result.as_ref().err().map(ToString::to_string)}));
                if result.is_ok() {watching.insert(root.path.clone());}
            }
        }
        state.diagnostics.watchers.lock().unwrap().retain(|id,_|roots.iter().any(|root|root.id==*id&&root.enabled));
        let current=roots.iter().map(|root|(root.id.clone(),format!("{}:{}:{}:{:?}",root.path,root.enabled,serde_json::to_string(&root.filters).unwrap_or_default(),root.exclusions))).collect::<HashMap<_,_>>();
        let changed=current.iter().filter(|(id,value)|fingerprints.get(*id)!=Some(*value)).map(|(id,_)|id.clone()).collect::<HashSet<_>>();
        if !settings.paused {
            if first || paused {let _=scan_all(state.clone(),false).await;last_scan=std::time::Instant::now();}
            else {for root in roots.iter().filter(|root|root.enabled&&changed.contains(&root.id)) {let _=scan_root_recorded(&state,root,false);}}
        }
        first=false;paused=settings.paused;fingerprints=current;
        tokio::select! {
            event=changes.recv()=>{
                if let Some(mut paths)=event {
                    tokio::select! {_=tokio::time::sleep(Duration::from_millis(750))=>{},_=state.shutdown.cancelled()=>break}
                    while paths.len()<10_000 {match changes.try_recv(){Ok(extra)=>paths.extend(extra),Err(_)=>break}}
                    if !paused {process_paths(&state,paths.into_iter().collect(),true);}
                }
            },
            _=state.wake.notified()=>{
                let hints=std::mem::take(&mut *state.diagnostics.hints.lock().unwrap());
                if !paused {process_paths(&state,hints,false);}
                // Queue-capacity backpressure may require another pass. A normal
                // file completion or model setting change does not walk all roots.
                if state.indexing_allowed() && state.store.pending_jobs().unwrap_or(5000)<500 {
                    for root in state.store.roots().unwrap_or_default().iter().filter(|root|root.enabled) {
                        if state.store.metadata::<bool>(&format!("scan_deferred:{}",root.id)).ok().flatten()==Some(true) {let _=scan_root_recorded(&state,root,false);}
                    }
                }
            },
            _=interval.tick()=>{
                let drops=state.diagnostics.watcher_drops.load(Ordering::Relaxed);
                if !paused && (last_scan.elapsed().as_secs()>=settings.reconcile_seconds.clamp(30,86400) || drops!=seen_drops) {
                    let _=scan_all(state.clone(),false).await;last_scan=std::time::Instant::now();seen_drops=drops;
                }
            },
            _=state.shutdown.cancelled()=>break,
        }
    }
    for result in [jobs.await,cleanup.await,embeddings.await,summaries.await] {if let Err(error)=result {tracing::error!(%error,"background worker failed during shutdown");}}
}

fn scan_root_recorded(state:&AppState,root:&Root,force:bool)->Result<()> {
    let result=scan_root(state,root,force);let _library=state.library_lock.lock().unwrap();
    if let Ok(mut latest)=state.store.root(&root.id) {if latest.path==root.path {latest.last_scan=Some(timestamp());latest.error=result.as_ref().err().map(ToString::to_string);state.store.save_root(&latest)?;}}
    result
}
fn process_paths(state:&AppState,paths:HashSet<PathBuf>,activity:bool) {
    let roots=state.store.roots().unwrap_or_default();
    for path in paths {
        if state.stopping.load(Ordering::SeqCst) {break;}
        let Some(root)=roots.iter().find(|root|root.enabled&&crate::rules::contains(Path::new(&root.path),&path)) else {continue;};
        if path.is_file() {
            if state.store.pending_jobs().unwrap_or(5000)>=5000 {
                let _=state.store.set_metadata(&format!("scan_deferred:{}",root.id),&true);
            } else if state.indexing_allowed() && queue_file(state,root,&path,false,false).unwrap_or(false) && activity {state.diagnostics.change(&root.id);}
        } else if path.is_dir() {
            if path==Path::new(&root.path) {continue;}
            let Ok(rules)=crate::rules::Rules::new(root,&state.home,&state.runtime.resources) else {continue;};
            for entry in WalkDir::new(&path).follow_links(false).max_depth(100).into_iter().filter_entry(|entry|!entry.file_type().is_dir()||rules.skip(entry.path(),true).is_none()).filter_map(Result::ok) {
                if state.store.pending_jobs().unwrap_or(5000)>=5000 {let _=state.store.set_metadata(&format!("scan_deferred:{}",root.id),&true);break;}
                if entry.file_type().is_file() {let _=queue_file(state,root,entry.path(),false,false);}
            }
        } else {
            // Renames/removals reconcile only records under the affected path.
            if let Ok(documents)=state.store.documents_under(&root.id,&path.to_string_lossy()) {
                for document in documents {if !Path::new(&document.path).is_file() {let _=state.store.schedule_cleanup_many(&[document.id]);}}
            }
        }
    }
}

async fn run_cleanup(state: Arc<AppState>) {
    let mut cursor=0i64; let mut revision=String::new();
    while !state.stopping.load(Ordering::SeqCst) {
        if !state.cleanup_allowed() {
            tokio::select! { _=tokio::time::sleep(Duration::from_secs(1))=>{}, _=state.shutdown.cancelled()=>break }
            continue;
        }
        let result = async {
            // Release maintenance access after each small transaction. A root
            // permission change or backup need not wait for a whole large purge.
            let _maintenance = state.maintenance.read().await;
            let _library = state.library_lock.lock().unwrap();
            if cleanup_step(&state)? { Ok(true) } else { discover_excluded(&state,&mut cursor,&mut revision) }
        }
        .await;
        match result {
            Ok(true) => tokio::task::yield_now().await,
            Ok(false) => {
                tokio::select! { _=tokio::time::sleep(Duration::from_millis(350))=>{}, _=state.shutdown.cancelled()=>break }
            }
            Err(error) => {
                tracing::warn!(%error, "bounded index cleanup deferred");
                tokio::select! { _=tokio::time::sleep(Duration::from_secs(2))=>{}, _=state.shutdown.cancelled()=>break }
            }
        }
    }
}

pub(crate) fn discover_excluded(state: &AppState, cursor: &mut i64, revision: &mut String) -> Result<bool> {
    let stamp=format!("{}:{}",state.store.metadata::<u64>("library_revision")?.unwrap_or(0),state.store.metadata::<String>("last_scan")?.unwrap_or_default());
    if *revision!=stamp { *revision=stamp; *cursor=0; }
    if *cursor==i64::MAX { return Ok(false); }
    let rows: Vec<(i64,String)>={
        let db=state.store.reader_budget(2)?;
        let mut statement=db.prepare("SELECT rowid,data FROM documents WHERE rowid>?1 ORDER BY rowid LIMIT 128")?;
        let rows=statement.query_map([*cursor],|row|Ok((row.get(0)?,row.get(1)?)))?.collect::<rusqlite::Result<_>>()?; rows
    };
    if rows.is_empty() { *cursor=i64::MAX; return Ok(false); }
    let roots=state.store.roots()?;
    let mut ids=vec![];
    for (_,text) in &rows {
        let document: Document=serde_json::from_str(text)?;
        if document.status=="removing" { continue; }
        let permitted=roots.iter().find(|root|root.id==document.root_id).is_some_and(|root|
            eligible(Path::new(&document.path),root,state).unwrap_or(false) && Path::new(&document.path).is_file());
        if !permitted { ids.push(document.id); }
    }
    state.store.schedule_cleanup_many(&ids)?;
    *cursor=rows.last().unwrap().0;
    Ok(!ids.is_empty())
}

pub(crate) fn cleanup_step(state: &AppState) -> Result<bool> {
    let Some(id) = state.store.next_cleanup()? else {
        return Ok(false);
    };
    let document = state.store.document(&id)?;
    let wanted = Path::new(&document.path).is_file()
        && state.store.root(&document.root_id).is_ok_and(|root| {
            root.enabled
                && Path::new(&document.path).canonicalize().is_ok_and(|path| {
                    Path::new(&root.path)
                        .canonicalize()
                        .is_ok_and(|root| path.starts_with(root))
                })
                && eligible(Path::new(&document.path), &root, state).unwrap_or(false)
        });
    if wanted {
        state.store.cancel_cleanup(&id)?;
        state.wake.notify_one();
    } else {
        state.store.remove_document_step(&id)?;
    }
    Ok(true)
}

async fn run_jobs(state: Arc<AppState>) {
    while !state.stopping.load(Ordering::SeqCst) {
        if !state.indexing_allowed() {
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        }
        let job = state.store.next_job_generation().unwrap_or(None);
        if let Some((path, root_id, force, attempts,generation)) = job {
            let _task=state.diagnostics.task("file_index",json!({"path":path,"root_id":root_id}));
            let _maintenance = state.maintenance.read().await;
            match index_file(&state, &path, &root_id, force).await {
                Ok(()) => {
                    let _ = state.store.finish_generation(&path, &root_id,generation);
                    if state
                        .store
                        .pending_jobs()
                        .is_ok_and(|count| count == 500 || count == 0)
                    {
                        state.wake.notify_one();
                    }
                }
                Err(error) => {
                    let _library = state.library_lock.lock().unwrap();
                    // Removing a collection or changing rules cancels an in-flight job.
                    // Do not resurrect its document or retry work for a new owner.
                    if !state.store.root(&root_id).is_ok_and(|root| root.enabled
                        && eligible(Path::new(&path), &root, &state).unwrap_or(false)) {
                        let _ = state.store.finish_generation(&path, &root_id,generation);
                        continue;
                    }
                    let message = crate::storage::error_message(&error);
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
                    if attempts >= 5
                        || message.contains("超过")
                        || message.contains("上限")
                        || message.contains("过大")
                    {
                        let _ = state.store.finish_generation(&path, &root_id,generation);
                    } else {
                        let _ = state.store.retry_generation(&path,generation,attempts);
                    }
                }
            }
        } else {
            tokio::time::sleep(Duration::from_millis(350)).await;
        }
    }
}

async fn index_file(state: &AppState, path: &str, root_id: &str, force: bool) -> Result<()> {
    let Ok(root) = state.store.root(root_id) else { return Ok(()); };
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
                state.store.schedule_cleanup_many(&[document.id])?;
            }
            return Ok(());
        }
    };
    if !canonical.starts_with(Path::new(&root.path).canonicalize()?) || !canonical.is_file() {
        return Ok(());
    }
    let metadata = tokio::fs::metadata(&canonical).await?;
    let settings = state.store.settings()?;
    let (mut document, previous) = {
        let _library = state.library_lock.lock().unwrap();
        let Ok(root) = state.store.root(root_id) else { return Ok(()); };
        if !root.enabled || !eligible(Path::new(path), &root, state)? { return Ok(()); }
        let previous = state.store.document_by_path(path)?;
    if !force
        && previous.as_ref().is_some_and(|document| {
            document.root_id == root_id && document.status == "ready"
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
    let project = state
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
        ocr: previous.as_ref().is_some_and(|document|document.ocr),
    };
    if let Some(previous) = &previous {
        document.version = previous.version.clone();
    }
    state.store.save_document(&document)?;
    (document, previous)
    };
    if metadata.len() > settings.max_file_mb.clamp(1, 32) * 1024 * 1024 {
        bail!("文件超过当前大小限制，可拆分后加入");
    }
    crate::storage::require_capacity(
        &state.home,
        &settings,
        metadata.len().saturating_mul(16).max(64 * 1024 * 1024),
    )?;
    let bytes = tokio::fs::read(&canonical).await?;
    let version = blake3::hash(&bytes).to_hex().to_string();
    if previous
        .as_ref()
        .is_some_and(|value| value.root_id == root_id && value.version == version && value.status == "ready")
        && !force
    {
        document.status = "ready".into();
        document.chunks = previous.unwrap().chunks;
        commit_indexed(state, &document, None)?;
        return Ok(());
    }
    let parsed = parse(state, &canonical, &settings).await?;
    if parsed.blocks.len() > 5000
        || parsed
            .blocks
            .iter()
            .map(|block| block.text.len())
            .sum::<usize>()
            > 16 * 1024 * 1024
    {
        bail!("解析结果超过安全上限，请拆分文档后加入");
    }
    let after = tokio::fs::read(&canonical).await?;
    if blake3::hash(&after).to_hex().as_str() != version {
        bail!("文件仍在保存，将在写入稳定后重试");
    }
    let after_metadata = tokio::fs::metadata(&canonical).await?;
    document.version = version;
    document.modified_ms = modified_ms(&after_metadata);
    document.size = after_metadata.len();
    document.ocr = parsed.ocr;
    let mut chunks: Vec<Chunk> = parsed
        .blocks
        .into_iter()
        .filter(|block| !block.text.trim().is_empty())
        .enumerate()
        .map(|(ordinal, block)| {
            let hash = blake3::hash(
                format!("chunk-v2:{}:{}:{}", document.id, document.version, ordinal).as_bytes(),
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
    let previous_chunks=state.store.chunks(&document.id)?;
    reuse_chunk_ids(&document.id,&previous_chunks,&mut chunks);
    document.chunks = chunks.len();
    document.status = if chunks.is_empty() { "empty" } else { "ready" }.into();
    if chunks.is_empty() {
        document.error=Some("未提取到可检索文字。图片型 Office 文档、嵌入图表或加密内容可能需要转换为 PDF 后使用 OCR。".into());
    }
    let growth = chunks
        .iter()
        .map(|chunk| chunk.text.len() as u64 * 16 + 48 * 1024)
        .sum::<u64>()
        .max(64 * 1024 * 1024);
    crate::storage::require_capacity(&state.home, &settings, growth)?;
    // CPU tokenization runs without the library or database write lock.
    let lexical = state.store.prepare_lexical(&document, &chunks)?;
    if commit_indexed(state, &document, Some((&chunks, &lexical)))? {
        tracing::info!(document_id=%document.id,bytes=document.size,chunks=chunks.len(),"document indexed");
    }
    Ok(())
}

pub(crate) fn commit_indexed(state: &AppState, document: &Document, content: Option<(&[Chunk], &[String])>) -> Result<bool> {
    let _library = state.library_lock.lock().unwrap();
    let Ok(root) = state.store.root(&document.root_id) else { return Ok(false); };
    if state.stopping.load(Ordering::SeqCst) || !root.enabled
        || !eligible(Path::new(&document.path), &root, state)? { return Ok(false); }
    let Ok(project) = state.store.project(&document.project_id) else { return Ok(false); };
    if project.archived || project.root_id.as_deref() != Some(root.id.as_str()) { return Ok(false); }
    let metadata = std::fs::metadata(&document.path)?;
    if metadata.len() != document.size || modified_ms(&metadata) != document.modified_ms {
        bail!("文件仍在保存，将在写入稳定后重试");
    }
    if let Some((chunks, lexical)) = content {
        state.store.replace_document_prepared(document, chunks, lexical)?;
    } else {
        state.store.save_document(document)?;
    }
    // Re-read after invalidating old evidence; never write the pre-parse project
    // snapshot over a newer user edit, archived state, or summary-stale flag.
    let mut project = state.store.project(&document.project_id)?;
    project.updated_at = timestamp();
    state.store.save_project(&project)?;
    state.store.set_metadata("last_indexed", &timestamp())?;
    Ok(true)
}

pub(crate) fn reuse_chunk_ids(document_id:&str,previous:&[Chunk],chunks:&mut[Chunk]) {
    let mut available=HashMap::<String,std::collections::VecDeque<&Chunk>>::new();
    for chunk in previous {available.entry(blake3::hash(chunk.text.as_bytes()).to_hex().to_string()).or_default().push_back(chunk);}
    let mut occurrences=HashMap::<String,usize>::new();
    let reserved=previous.iter().map(|chunk|chunk.id.clone()).collect::<HashSet<_>>();
    let mut assigned=HashSet::new();
    for chunk in chunks {
        let digest=blake3::hash(chunk.text.as_bytes()).to_hex().to_string();
        let ordinal=occurrences.entry(digest.clone()).or_default();
        chunk.id=if let Some(old)=available.get_mut(&digest).and_then(|items|items.pop_front()).filter(|old|old.text==chunk.text) {old.id.clone()}
            else {loop {
                let candidate=blake3::hash(format!("chunk-v3:{document_id}:{digest}:{ordinal}").as_bytes()).to_hex().to_string();
                if !reserved.contains(&candidate) && !assigned.contains(&candidate) {break candidate;}
                *ordinal+=1;
            }};
        assigned.insert(chunk.id.clone());
        *ordinal+=1;
    }
}

pub(crate) fn queue_file(state:&AppState,root:&Root,path:&Path,force:bool,immediate:bool)->Result<bool> {
    #[cfg(windows)]
    let canonical=path.canonicalize()?;
    #[cfg(windows)]
    let path=canonical.as_path();
    if !root.enabled || !path.is_file() || !eligible(path,root,state)? {return Ok(false);}
    let metadata=std::fs::metadata(path)?;let text=path.to_string_lossy();let previous=state.store.document_by_path(&text)?;
    if !force && previous.as_ref().is_some_and(|document|document.status=="ready" && document.root_id==root.id && document.modified_ms==modified_ms(&metadata) && document.size==metadata.len()) {return Ok(false);}
    let observed=format!("{}:{}",modified_ms(&metadata),metadata.len());
    let quiet=if force || immediate {0}else if previous.is_some(){state.store.settings()?.file_quiet_seconds}else{2};
    state.store.enqueue_observed(&text,&root.id,force,&observed,quiet)
}

pub fn find_project(path: &Path, root: &Path) -> PathBuf {
    let mut directory = path.parent();
    let mut manifest = None;
    let mut workspace = None;
    while let Some(current) = directory {
        if !current.starts_with(root) {
            break;
        }
        // A Git worktree/submodule may use a .git file rather than a directory.
        if current.join(".git").exists() {
            return current.into();
        }
        let markers = [
            "package.json",
            "pyproject.toml",
            "Cargo.toml",
            "go.mod",
            "pom.xml",
            "build.gradle",
            "build.gradle.kts",
            "CMakeLists.txt",
            "composer.json",
            "Gemfile",
            "mix.exs",
        ];
        let identified = markers.iter().any(|name| current.join(name).is_file())
            || std::fs::read_dir(current).ok().is_some_and(|entries| {
                entries.take(256).flatten().any(|entry| {
                    entry
                        .path()
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| matches!(extension, "sln" | "slnx" | "csproj"))
                })
            });
        if identified && manifest.is_none() {
            manifest = Some(current.to_path_buf());
        }
        if current.join("pnpm-workspace.yaml").is_file() || current.join("lerna.json").is_file() {
            workspace = Some(current.to_path_buf());
        }
        for name in ["package.json", "Cargo.toml"] {
            let file = current.join(name);
            if std::fs::metadata(&file).is_ok_and(|metadata| metadata.len() < 65536) {
                if let Ok(bytes) = std::fs::read(&file) {
                    if name == "package.json"
                        && serde_json::from_slice::<serde_json::Value>(&bytes)
                            .is_ok_and(|value| value.get("workspaces").is_some())
                        || name == "Cargo.toml"
                            && String::from_utf8_lossy(&bytes).contains("[workspace]")
                    {
                        workspace = Some(current.to_path_buf());
                    }
                }
            }
        }
        if current == root {
            break;
        }
        directory = current.parent();
    }
    if let Some(project) = workspace.or(manifest) {
        return project;
    }
    // Unmarked folders remain separate collections when watching Desktop or a
    // parent folder. Files directly in the watched root belong to that root.
    if let Ok(relative) = path.strip_prefix(root) {
        if relative.components().count() > 1 {
            if let Some(first) = relative.components().next() {
                return root.join(first.as_os_str());
            }
        }
    }
    root.into()
}

async fn parse(state: &AppState, path: &Path, settings: &Settings) -> Result<ParsedFile> {
    state.parser.parse(path, settings).await
}

async fn run_embeddings(state: Arc<AppState>) {
    let mut cursor = 0;
    let mut cycle = String::new();
    let mut last_indexed = String::new();
    let mut caught_up = false;
    while !state.stopping.load(Ordering::SeqCst) {
        let settings = state.store.settings().unwrap_or_default();
        let protected = crate::progress::snapshot(&state, &settings)
            .map(|progress| progress.protected)
            .unwrap_or(true);
        if settings.embedding_mode == "disabled"
            || protected
            || Models::remote_embeddings(&settings)
        {
            state.models.unload_local().await;
        }
        if !state.indexing_allowed() || !state.models.embedding_available(&settings) || protected {
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }
        let _maintenance = state.maintenance.read().await;
        let model = state.models.embedding_model(&settings);
        let roots = state
            .store
            .roots()
            .unwrap_or_default()
            .into_iter()
            .map(|root| (root.id, root.enabled, root.policy))
            .collect::<Vec<_>>();
        let key = format!(
            "{model}:{}",
            serde_json::to_string(&roots).unwrap_or_default()
        );
        let stamp = state
            .store
            .metadata::<String>("last_indexed")
            .ok()
            .flatten()
            .unwrap_or_default();
        if key != cycle || caught_up && stamp != last_indexed {
            cursor = 0;
            caught_up = false;
            cycle = key;
        }
        if caught_up {
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }
        let (next, chunks, complete) = match state.store.pending_embeddings(
            &model,
            if Models::remote_embeddings(&settings) {
                settings.api_embedding_batch_size
            } else {
                1
            },
            Models::remote_embeddings(&settings),
            cursor,
        ) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(error=%error,"pending vector selection failed");
                *state.semantic_error.write().await = Some(error.to_string());
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };
        if complete {
            caught_up = true;
            last_indexed = stamp;
            continue;
        }
        if chunks.is_empty() {
            cursor = next;
            continue;
        }
        state.embedding_busy.store(true, Ordering::SeqCst);
        let texts = chunks
            .iter()
            .map(|chunk| chunk.text.clone())
            .collect::<Vec<_>>();
        let started = std::time::Instant::now();
        let revision=state.store.metadata::<u64>("library_revision").ok().flatten().unwrap_or(0);
        let _task=state.diagnostics.task("embedding",json!({"model":model,"batch_chunks":chunks.len(),"profile":settings.indexing_profile}));
        let mut calculation = Box::pin(state.models.embed(&settings, &texts, false));
        let result = loop {
            tokio::select! {
                result = &mut calculation => break Some(result),
                _ = state.shutdown.cancelled() => break None,
                _ = tokio::time::sleep(Duration::from_millis(500)) => {
                    let current = state.store.settings().unwrap_or_default();
                    if current.paused || !state.indexing_allowed()
                        || state.store.metadata::<u64>("library_revision").ok().flatten().unwrap_or(0)!=revision
                        || current.embedding_mode != settings.embedding_mode
                        || current.indexing_profile != settings.indexing_profile
                        || state.models.embedding_model(&current) != model {
                        break None;
                    }
                }
            }
        };
        // Drop the startup/request future before taking the model mutex.
        drop(calculation);
        let Some(result) = result else {
            if !Models::remote_embeddings(&settings) {
                state.models.unload_local().await;
            }
            state.embedding_busy.store(false, Ordering::SeqCst);
            continue;
        };
        match result {
            Ok(vectors) => {
                let current = state.store.settings().unwrap_or_default();
                let mut saved = !current.paused && state.models.embedding_model(&current) == model;
                let mut save_error = None;
                if saved {
                    let _library = state.library_lock.lock().unwrap();
                    saved = state.store.metadata::<u64>("library_revision").ok().flatten().unwrap_or(0) == revision;
                    if saved {
                        for (chunk, vector) in chunks.iter().zip(vectors) {
                            if crate::storage::require_capacity(&state.home, &current, 2 * 1024 * 1024).is_err() {
                                saved = false;
                                break;
                            }
                            if let Ok(document) = state.store.document(&chunk.document_id) {
                                if state.check_document(&Scope::Owner, &document, Models::remote_embeddings(&settings)).is_ok()
                                    && state.store.has_chunk(&document.id, &chunk.id) {
                                    if let Err(error) = state.store.save_embedding(&chunk.id, &model, &vector) {
                                        saved = false;
                                        save_error = Some(error.to_string());
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(error) = save_error { *state.semantic_error.write().await = Some(error); }
                if saved {
                    cursor = next;
                    {
                        let mut metrics = state.embedding_metrics.lock().unwrap();
                        if metrics.model != model {
                            *metrics = crate::progress::EmbeddingMetrics::default();
                            metrics.model = model.clone();
                        }
                        metrics.processed += chunks.len() as u64;
                        metrics.seconds += started.elapsed().as_secs_f64()
                            + if settings.indexing_profile == "low" {
                                0.5
                            } else {
                                0.15
                            };
                    }
                    *state.semantic_error.write().await = None;
                }
            }
            Err(error) => {
                tracing::warn!(error=%error,"background vector calculation failed");
                *state.semantic_error.write().await = Some(error.to_string());
                tokio::select! {_=tokio::time::sleep(Duration::from_secs(5))=>{},_=state.shutdown.cancelled()=>{}}
            }
        }
        state.embedding_busy.store(false, Ordering::SeqCst);
        let delay = match settings.indexing_profile.as_str() {
            "low" => 500,
            "performance" => 0,
            _ => 150,
        };
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
    }
}

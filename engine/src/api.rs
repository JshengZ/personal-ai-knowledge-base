use crate::{backup, connections, indexer, knowledge, retrieval, state::AppState, types::*};
use anyhow::{Context, Result};
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, Request, State, Extension},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use rusqlite::OptionalExtension;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{atomic::Ordering, Arc},
};

#[derive(Debug)]
pub struct ApiError(pub anyhow::Error);
impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(error: E) -> Self {
        Self(error.into())
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let error=crate::errors::describe(&self.0);
        (error.status,Json(error.value())).into_response()
    }
}
type ApiResult = std::result::Result<Json<Value>, ApiError>;

macro_rules! bail {
    ($($argument:tt)*) => { return Err(anyhow::anyhow!($($argument)*).into()) };
}

#[derive(Clone)]
pub struct ApiState {
    pub app: Arc<AppState>,
    pub token: String,
    pub port: u16,
}

async fn owner_auth(State(state): State<ApiState>, request: Request, next: Next) -> Response {
    if !origin_allowed(request.headers()) {
        return (StatusCode::FORBIDDEN, Json(json!({"error":"来源不可用"}))).into_response();
    }
    let supplied = request
        .headers()
        .get("authorization")
        .and_then(|header| header.to_str().ok())
        .and_then(|text| text.strip_prefix("Bearer "));
    if supplied != Some(state.token.as_str()) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"本地会话未授权"})),
        )
            .into_response();
    }
    next.run(request).await
}

pub fn origin_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get("origin") else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Ok(url) = url::Url::parse(origin) else {
        return false;
    };
    matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
        && matches!(url.scheme(), "http" | "https")
}

pub fn router(app: Arc<AppState>, token: String, port: u16) -> Router {
    let state = ApiState { app, token, port };
    let owner = Router::new()
        .route("/health", get(health))
        .route("/status", get(service_status))
        .route("/state", get(snapshot))
        .route("/documents", get(documents))
        .route("/roots", post(add_root))
        .route("/roots/batch", post(add_roots))
        .route("/roots/inspect", post(inspect_roots))
        .route("/sources/{id}/exclude", post(exclude_source))
        .route("/roots/{id}", patch(update_root).delete(remove_root))
        .route("/scan", post(scan))
        .route("/projects/{id}", patch(update_project))
        .route("/projects/{id}/summarize", post(summarize_project))
        .route("/sources/{id}", get(source))
        .route("/search", post(search))
        .route("/ask", post(ask))
        .route("/memories", post(propose))
        .route("/memories/{id}", patch(update_memory).delete(remove_memory))
        .route("/settings", patch(save_settings))
        .route("/appearance", get(get_appearance).patch(save_appearance))
        .route("/onboarding", get(get_onboarding).patch(save_onboarding))
        .route("/storage/validate", post(validate_storage))
        .route("/models/test/{kind}", post(test_model))
        .route("/connections", post(create_connection))
        .route("/connections/{id}", patch(update_connection))
        .route("/connections/{id}/config", get(connection_config))
        .route("/connections/{id}/install-zcode", post(install_zcode))
        .route("/connections/{id}/test", post(test_connection))
        .route("/backups/export", get(export_backup))
        .route("/backups/restore", post(restore_backup))
        .route("/shutdown", post(shutdown))
        .layer(middleware::from_fn_with_state(state.clone(), owner_auth))
        .layer(DefaultBodyLimit::max(512 * 1024 * 1024));
    Router::new()
        .nest("/api", owner)
        .route("/agent/{id}/{tool}", post(agent_tool))
        .route("/mcp/{id}", axum::routing::any(crate::mcp::http_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            isolate_requests,
        ))
        .layer(middleware::from_fn(crate::queries::track))
        .with_state(state)
}

async fn isolate_requests(State(state): State<ApiState>, request: Request, next: Next) -> Response {
    // Keep protocol handshakes and health checks on the async runtime. Database
    // and parsing work runs on a bounded set of separate blocking threads.
    let path = request.uri().path();
    if path == "/api/health"
        || path == "/api/shutdown"
        || path.starts_with("/mcp/")
        || path.starts_with("/agent/")
        || path=="/api/status"
        || path=="/api/search"
        || !path.starts_with("/api/") && !path.starts_with("/agent/")
    {
        return next.run(request).await;
    }
    // Only short read requests have the 12-second responsiveness budget.
    // A timed-out spawn_blocking task keeps running: imposing that deadline on
    // mutations produced false failures followed by invisible late writes.
    // Model requests have their own HTTP timeout; backups must finish atomically.
    let short_read = (request.method() == axum::http::Method::GET
        && path != "/api/backups/export")
        || path == "/api/search"
        || (path.starts_with("/agent/") && !path.ends_with("/propose_memory"));
    let lanes=if short_read {state.app.read_requests.clone()}else{state.app.requests.clone()};
    let Ok(permit) = lanes.try_acquire_owned() else {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":"同时处理的界面请求已达上限，请稍后重试。","code":"ui_request_capacity"}))).into_response();
    };
    let handle = tokio::runtime::Handle::current();
    let work = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        handle.block_on(next.run(request))
    });
    if !short_read {
        return match work.await {
            Ok(response) => response,
            Err(error) => {
                tracing::error!(%error, "request worker failed");
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":"请求处理失败，请查看处理记录。"}))).into_response()
            }
        };
    }
    match tokio::time::timeout(std::time::Duration::from_secs(12), work).await {
        Ok(Ok(response)) => response,
        Ok(Err(error))=>{tracing::error!(%error,"read worker failed");(StatusCode::INTERNAL_SERVER_ERROR,Json(json!({"error":"读取请求未能完成，请查看处理记录。","code":"read_worker_failed"}))).into_response()},
        Err(_) => (StatusCode::GATEWAY_TIMEOUT, Json(json!({"error":"读取资料超过执行时限，请重试或查看运行状态。","code":"ui_read_timeout","details":{"deadline_seconds":12}}))).into_response(),
    }
}

async fn health() -> Json<Value> {
    Json(json!({"ok":true,"version":env!("CARGO_PKG_VERSION"),"role":"service","pid":std::process::id(),"sqlite_version":rusqlite::version()}))
}

async fn service_status(State(state):State<ApiState>)->ApiResult {
    let app=state.app.clone();
    let mut status=app.diagnostics.snapshot();
    status["queries"]=json!({"active":6-app.query_lanes.available_permits(),"capacity":6,"keyword_active":2-app.keyword_lanes.available_permits(),"keyword_capacity":2,"deadline_seconds":crate::queries::QUERY_SECONDS,"bridge_deadline_seconds":crate::queries::BRIDGE_SECONDS});
    status["maintenance"]=json!(app.maintenance.try_read().is_err());
    let metrics=tokio::task::spawn_blocking(move||->Result<Value> {let settings=app.store.settings()?;Ok(json!({"queue":app.store.job_status()?,"semantic":crate::progress::snapshot(&app,&settings)?,"paused":settings.paused,"quiet_seconds":settings.file_quiet_seconds,"reconcile_seconds":settings.reconcile_seconds}))}).await?;
    match metrics {Ok(metrics)=>status["index"]=metrics,Err(error)=>status["index_error"]=json!(error.to_string())};
    Ok(Json(status))
}

async fn snapshot(State(state): State<ApiState>) -> ApiResult {
    let app = &state.app;
    let _maintenance=app.maintenance.read().await;
    let mut documents = app.store.recent_documents(100)?;
    documents.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let settings = app.store.settings()?;
    let semantic_error = app.semantic_error.read().await.clone();
    let mut stats = app.store.stats()?;
    stats["projects"]=json!(app.store.projects()?.len());
    let mut discovered = 0u64;
    let mut format_warnings = vec![];
    for root in app.store.roots()?.iter().filter(|root| root.enabled) {
        discovered += app
            .store
            .metadata::<u64>(&format!("scan_found:{}", root.id))?
            .unwrap_or(0);
        for (extension, count) in app
            .store
            .metadata::<HashMap<String, u64>>(&format!("scan_formats:{}", root.id))?
            .unwrap_or_default()
        {
            format_warnings.push(json!({"root_id":root.id,"root_name":root.name,"extension":extension,"count":count}));
        }
    }
    stats["discovered"] = json!(discovered
        .max(stats["documents"].as_u64().unwrap_or(0) + stats["pending"].as_u64().unwrap_or(0)));
    let visible_roots=app.store.roots()?;
    let visible_rules=visible_roots.iter().filter_map(|root|crate::rules::Rules::new(root,&app.home,&app.runtime.resources).ok().map(|rules|(root.id.clone(),rules))).collect::<HashMap<_,_>>();
    let documents=documents.into_iter().filter(|document|document.status!="removing"&&visible_rules.get(&document.root_id).is_some_and(|rules|rules.skip_indexed(std::path::Path::new(&document.path)).is_none())).take(100).collect::<Vec<_>>();
    Ok(Json(
        json!({"version":env!("CARGO_PKG_VERSION"),"roots":app.store.roots()?,"projects":app.store.projects()?,"archived_projects":app.store.all_projects()?.into_iter().filter(|project|project.archived).collect::<Vec<_>>(),"library_revision":app.store.metadata::<u64>("library_revision")?.unwrap_or(0),"scan_summaries":app.store.roots()?.iter().map(|root| Ok(json!({"root_id":root.id,"summary":app.store.metadata::<Value>(&format!("scan_summary:{}",root.id))?}))).collect::<Result<Vec<_>>>()?,
        "documents":documents,"memories":app.store.memories()?,
        "connections":app.store.connections()?,"stats":stats,"settings":settings,"onboarding":crate::onboarding::load(&app.store)?,
        "events":app.store.events(60)?,"last_indexed":app.store.metadata::<String>("last_indexed")?,
        "last_scan":app.store.metadata::<String>("last_scan")?,"scanning":app.scanning.load(Ordering::SeqCst),
        "embedding_busy":app.embedding_busy.load(Ordering::SeqCst),"embedding_available":app.models.embedding_available(&settings),
        "embedding_model":app.models.embedding_model(&settings),"semantic_error":semantic_error,"data_directory":app.home,
        "storage":crate::storage::status(&app.home,&settings,64*1024*1024)?,
        "semantic_progress":crate::progress::snapshot(app,&settings)?,
        "format_warnings":format_warnings,"appearance":crate::appearance::load(&app.store)?}),
    ))
}

async fn documents(
    State(state): State<ApiState>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult {
    let _maintenance=state.app.maintenance.read().await;
    let roots=state.app.store.roots()?;
    let rules=roots.iter().filter_map(|root|crate::rules::Rules::new(root,&state.app.home,&state.app.runtime.resources).ok().map(|rules|(root.id.clone(),rules))).collect::<HashMap<_,_>>();
    let mut documents = state.app.store.documents()?;
    documents.retain(|document| {
        if document.status == "removing" || !rules.get(&document.root_id).is_some_and(|rules|rules.skip_indexed(std::path::Path::new(&document.path)).is_none()) { return false; }
        if query.get("collection_id").is_some_and(|id|document.root_id!=*id) { return false; }
        query
            .get("project_id")
            .is_none_or(|id| document.project_id == *id)
            && query
                .get("query")
                .is_none_or(|term| document.title.to_lowercase().contains(&term.to_lowercase()))
            && query
                .get("status")
                .is_none_or(|status| document.status == *status)
    });
    documents.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(||a.id.cmp(&b.id)));
    let total = documents.len();
    let offset = query
        .get("offset")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_default();
    let limit = query
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(100)
        .clamp(1, 200);
    Ok(Json(
        json!({"documents":documents.into_iter().skip(offset).take(limit).collect::<Vec<_>>(),"total":total}),
    ))
}

fn root_candidate(app: &AppState, input: &Value) -> Result<Root> {
    crate::storage::require_capacity(&app.home, &app.store.settings()?, 64 * 1024 * 1024)?;
    let raw = input["path"].as_str().context("请选择资料目录")?;
    let path = std::path::Path::new(raw)
        .canonicalize()
        .context("目录无法访问，请重新选择")?;
    if !path.is_dir() {
        bail!("请选择一个文件夹");
    }
    if path.parent().is_none() {
        bail!("请选择具体的项目或资料文件夹，不支持直接索引整块磁盘");
    }
    if path.starts_with(&app.home) || app.home.starts_with(&path) {
        bail!("知识库的数据目录已由应用管理，请选择原始资料目录");
    }
    let path = path.to_string_lossy().into_owned();
    if app.store.roots()?.iter().any(|root| {
        crate::rules::contains(std::path::Path::new(&root.path),std::path::Path::new(&path))
            || crate::rules::contains(std::path::Path::new(&path),std::path::Path::new(&root.path))
    }) {
        bail!("这个目录与已有目录重叠，请直接管理已添加的目录");
    }
    let policy = input["policy"].as_str().unwrap_or("agents");
    if !["local", "company", "agents"].contains(&policy) {
        bail!("目录权限无效");
    }
    let exclusions: Vec<String> = serde_json::from_value(input["exclusions"].clone())
        .unwrap_or_else(|_| vec!["**/dist/**".into(), "**/*.min.js".into(), "**/.env*".into()]);
    indexer::validate_exclusions(&exclusions)?;
    let name = input["name"]
        .as_str()
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            std::path::Path::new(&path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
    if name.is_empty() || name.chars().count()>100 { bail!("集合名称应为 1–100 字"); }
    let root = Root {
        id: identifier(),
        path,
        name,
        policy: policy.into(),
        enabled: true,
        exclusions,
        filters: input.get("filters").map(|value|serde_json::from_value(value.clone())).transpose()?.unwrap_or_default(),
        created_at: timestamp(),
        last_scan: None,
        error: None,
    };
    crate::rules::validate(&root)?;
    Ok(root)
}

fn add_root_impl(app: &AppState, input: &Value) -> Result<Root> {
    let root = root_candidate(app,input)?;
    app.store.save_root(&root)?;
    crate::collections::touch(&app.store)?;
    app.wake.notify_one();
    Ok(root)
}
async fn add_roots(State(state): State<ApiState>, Json(input): Json<Value>) -> ApiResult {
    let _maintenance=state.app.maintenance.read().await;
    let _library=state.app.library_lock.lock().unwrap();
    let items=input["items"].as_array().context("请选择资料目录")?;
    if items.is_empty() || items.len()>50 { bail!("每次可添加 1–50 个资料目录"); }
    let mut added=vec![]; let mut errors=vec![];
    let mut sorted=items.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|item|std::path::Path::new(item["path"].as_str().unwrap_or_default()).canonicalize().map(|path|path.components().count()).unwrap_or(usize::MAX));
    for item in sorted {
        match add_root_impl(&state.app,item) {
            Ok(root)=>added.push(root),
            Err(error)=>errors.push(json!({"path":item["path"],"name":item["name"],"error":error.to_string()})),
        }
    }
    Ok(Json(json!({"added":added,"errors":errors})))
}
async fn inspect_roots(State(state): State<ApiState>, Json(input): Json<Value>) -> ApiResult {
    let items=input["items"].as_array().context("请选择资料目录")?;
    if items.len()>50 { bail!("每次最多检查 50 个目录"); }
    let mut results=vec![];
    for item in items {
        match root_candidate(&state.app,item) {
            Ok(root)=>{
                let rules=crate::rules::Rules::new(&root,&state.app.home,&state.app.runtime.resources)?;
                let directory=std::path::Path::new(&root.path);
                let project=[".git","package.json","pyproject.toml","Cargo.toml","go.mod"].iter().any(|name|directory.join(name).exists());
                let advice=project.then_some("检测到代码工程。依赖与构建产物默认排除，修改文件会在静默后更新；规则可按需要调整。");
                results.push(json!({"path":item["path"],"canonical_path":root.path,"name":root.name,"warning":rules.skip(directory,true),"advice":advice}));
            },
            Err(error)=>results.push(json!({"path":item["path"],"error":error.to_string()})),
        }
    }
    let canonical=results.iter().map(|item|item["canonical_path"].as_str().map(str::to_string)).collect::<Vec<_>>();
    for index in 0..results.len() {
        if let Some(path)=&canonical[index] {
            if canonical.iter().enumerate().any(|(other,parent)|other!=index && parent.as_ref().is_some_and(|parent|crate::rules::contains(std::path::Path::new(parent),std::path::Path::new(path)) && (parent!=path || other<index))) {
                results[index]["error"]=json!("与待添加的父目录或重复目录重叠，请保留一个范围");
            }
        }
    }
    Ok(Json(json!({"items":results})))
}
async fn exclude_source(State(state): State<ApiState>, Path(id): Path<String>, Json(input): Json<Value>) -> ApiResult {
    let _maintenance=state.app.maintenance.read().await;
    let _library=state.app.library_lock.lock().unwrap();
    let document=state.app.store.document(&id)?;
    let mut root=state.app.store.root(&document.root_id)?;
    let relative=crate::rules::relative(std::path::Path::new(&root.path),std::path::Path::new(&document.path)).context("文件不在集合内")?;
    match input["scope"].as_str().unwrap_or("file") {
        "file"=>root.filters.excluded_paths.push(relative),
        "directory"=>root.filters.excluded_paths.push(relative.rsplit_once('/').map(|(parent,_)|parent.to_string()).unwrap_or(".".into())),
        "extension"=>root.exclusions.push(format!("**/*.{}",document.extension)),
        _=>bail!("排除范围无效"),
    }
    root.filters.excluded_paths.sort();root.filters.excluded_paths.dedup();
    root.exclusions.sort();root.exclusions.dedup();
    crate::rules::validate(&root)?;
    state.app.store.save_root(&root)?;
    crate::collections::touch(&state.app.store)?;
    state.app.wake.notify_one();
    Ok(Json(json!({"ok":true,"root":root})))
}

async fn validate_storage(State(state): State<ApiState>, Json(input): Json<Value>) -> ApiResult {
    let raw = input["path"].as_str().context("请选择新的数据目录")?;
    let destination =
        crate::storage::validate_destination(&state.app.home, std::path::Path::new(raw))?;
    Ok(Json(json!({"path":destination})))
}

async fn add_root(State(state): State<ApiState>, Json(input): Json<Value>) -> ApiResult {
    let _maintenance = state.app.maintenance.read().await;
    let _library=state.app.library_lock.lock().unwrap();
    Ok(Json(json!(add_root_impl(&state.app, &input)?)))
}

async fn update_root(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(input): Json<Value>,
) -> ApiResult {
    let app = &state.app;
    let _maintenance = app.maintenance.read().await;
    let _library=app.library_lock.lock().unwrap();
    let mut root = app.store.root(&id)?;
    if let Some(path) = input["path"].as_str() {
        let canonical = std::path::Path::new(path)
            .canonicalize()
            .context("新目录无法访问")?;
        if !canonical.is_dir() {
            bail!("请选择文件夹");
        }
        if canonical.starts_with(&app.home) || app.home.starts_with(&canonical) {
            bail!("请选择知识库数据目录之外的原始资料目录");
        }
        if app.store.roots()?.iter().any(|other| {
            other.id != id
                && (canonical.starts_with(&other.path)
                    || std::path::Path::new(&other.path).starts_with(&canonical))
        }) {
            bail!("这个目录与已有目录重叠");
        }
        if canonical.to_string_lossy() != root.path {
            for document in app
                .store
                .documents()?
                .into_iter()
                .filter(|document| document.root_id == id)
            {
                app.store.schedule_cleanup_many(&[document.id])?;
            }
            root.path = canonical.to_string_lossy().into_owned();
            root.last_scan = None;
            root.error = None;
            for mut project in app.store.all_projects()?.into_iter().filter(|project|project.root_id.as_deref()==Some(id.as_str())) { project.archived=true; app.store.save_project(&project)?; }
        }
    }
    if let Some(name) = input["name"].as_str() {
        if name.trim().is_empty() || name.trim().chars().count()>100 {
            bail!("名称不能为空");
        }
        root.name = name.trim().into();
    }
    if let Some(policy) = input["policy"].as_str() {
        if !["local", "company", "agents"].contains(&policy) {
            bail!("权限无效");
        }
        root.policy = policy.into();
    }
    if let Some(enabled) = input["enabled"].as_bool() {
        root.enabled = enabled;
    }
    if input.get("exclusions").is_some() {
        let patterns: Vec<String> = serde_json::from_value(input["exclusions"].clone())?;
        indexer::validate_exclusions(&patterns)?;
        root.exclusions = patterns;
    }
    if let Some(filters)=input.get("filters") { root.filters=serde_json::from_value(filters.clone())?; }
    crate::rules::validate(&root)?;
    app.store.save_root(&root)?;
    crate::collections::touch(&app.store)?;
    app.wake.notify_one();
    Ok(Json(json!(root)))
}

async fn remove_root(State(state): State<ApiState>, Path(id): Path<String>) -> ApiResult {
    let _maintenance=state.app.maintenance.read().await;
    let _library=state.app.library_lock.lock().unwrap();
    state.app.store.root(&id)?;
    crate::collections::remove(&state.app.store,&id)?;
    state.app.wake.notify_one();
    Ok(Json(json!({"ok":true})))
}

async fn scan(State(state): State<ApiState>, Json(input): Json<Value>) -> ApiResult {
    let app = state.app.clone();
    let force = input["force"].as_bool().unwrap_or(false);
    tokio::spawn(async move {
        let _ = indexer::scan_all(app, force).await;
    });
    Ok(Json(json!({"ok":true})))
}

async fn update_project(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(input): Json<Value>,
) -> ApiResult {
    let _maintenance = state.app.maintenance.read().await;
    let _library=state.app.library_lock.lock().unwrap();
    let mut project = state.app.store.project(&id)?;
    for (key, target) in [
        ("name", &mut project.name),
        ("goal", &mut project.goal),
        ("approach", &mut project.approach),
    ] {
        if let Some(text) = input[key].as_str() {
            if text.chars().count() > 4000 {
                bail!("内容过长");
            }
            *target = text.trim().into();
        }
    }
    if project.name.is_empty() {
        bail!("项目名称不能为空");
    }
    if input.get("decisions").is_some() {
        project.decisions = serde_json::from_value(input["decisions"].clone())?;
    }
    project.summary_evidence.clear();
    project.summary_stale = false;
    project.updated_at = timestamp();
    state.app.store.save_project(&project)?;
    if let Some(id)=&project.root_id { if let Ok(mut root)=state.app.store.root(id) { if crate::rules::normalized(std::path::Path::new(&root.path))==crate::rules::normalized(std::path::Path::new(&project.root_path)) { root.name=project.name.clone();state.app.store.save_root(&root)?; } } }
    crate::collections::touch(&state.app.store)?;
    Ok(Json(json!(project)))
}

async fn summarize_project(State(state): State<ApiState>, Path(id): Path<String>) -> ApiResult {
    Ok(Json(json!(knowledge::summarize(&state.app, &id).await?)))
}
async fn source(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult {
    let request = retrieval::ReadRequest {
        document_id: id,
        chunk_id: query.get("chunk_id").cloned(),
        offset: query.get("offset").and_then(|value| value.parse().ok()),
        limit: Some(30),
    };
    Ok(Json(retrieval::read_source(
        &state.app,
        &Scope::Owner,
        &request,
    )?))
}
async fn search(
    State(state): State<ApiState>,
    Json(input): Json<retrieval::SearchRequest>,
) -> ApiResult {
    Ok(Json(json!(
        crate::queries::run(state.app.clone(),Scope::Owner,"owner_search".into(),serde_json::to_value(input)?,crate::queries::RequestLife::new()).await?
    )))
}
async fn ask(
    State(state): State<ApiState>,
    Json(input): Json<retrieval::SearchRequest>,
) -> ApiResult {
    Ok(Json(retrieval::ask(&state.app, &input).await?))
}
async fn propose(
    State(state): State<ApiState>,
    Json(input): Json<retrieval::ProposeRequest>,
) -> ApiResult {
    let _maintenance = state.app.maintenance.read().await;
    Ok(Json(json!(retrieval::propose(
        &state.app,
        &Scope::Owner,
        input
    )?)))
}
async fn update_memory(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(input): Json<Value>,
) -> ApiResult {
    let _maintenance = state.app.maintenance.read().await;
    Ok(Json(json!(knowledge::update_memory(
        &state.app, &id, input
    )?)))
}
async fn remove_memory(State(state): State<ApiState>, Path(id): Path<String>) -> ApiResult {
    let _maintenance = state.app.maintenance.read().await;
    state.app.store.delete_memory(&id)?;
    Ok(Json(json!({"ok":true})))
}

async fn get_appearance(State(state): State<ApiState>) -> ApiResult {
    Ok(Json(json!(crate::appearance::load(&state.app.store)?)))
}

async fn get_onboarding(State(state): State<ApiState>) -> ApiResult {
    Ok(Json(json!(crate::onboarding::load(&state.app.store)?)))
}

async fn save_onboarding(State(state): State<ApiState>, Json(input): Json<crate::onboarding::Update>) -> ApiResult {
    let _maintenance = state.app.maintenance.read().await;
    Ok(Json(json!(crate::onboarding::update(&state.app.store, input)?)))
}

async fn save_appearance(State(state): State<ApiState>, Json(input): Json<Value>) -> ApiResult {
    let _maintenance = state.app.maintenance.read().await;
    Ok(Json(json!(crate::appearance::update(&state.app.store, input)?)))
}

async fn save_settings(State(state): State<ApiState>, Json(mut input): Json<Value>) -> ApiResult {
    let app = &state.app;
    let _maintenance = app.maintenance.read().await;
    let old = app.store.settings()?;
    let mut current = serde_json::to_value(&old)?;
    let secrets = input
        .as_object_mut()
        .and_then(|object| object.remove("keys"))
        .unwrap_or_else(|| json!({}));
    let changes = input.as_object().context("设置格式无效")?;
    for (key, value) in changes {
        if ["chat", "embedding", "rerank"].contains(&key.as_str()) {
            if let Some(fields) = value.as_object() {
                for (field, value) in fields {
                    if !["has_key", "credential_id"].contains(&field.as_str()) {
                        current[key][field] = value.clone();
                    }
                }
            }
        } else {
            current[key] = value.clone();
        }
    }
    let mut settings: Settings = serde_json::from_value(current)?;
    for provider in [&settings.chat, &settings.embedding, &settings.rerank] {
        crate::models::Models::validate_provider(provider)?;
    }
    if !["auto", "local", "disabled"].contains(&settings.embedding_mode.as_str()) {
        bail!("向量模式无效");
    }
    if !["openai", "anthropic"].contains(&settings.chat.protocol.as_str()) {
        bail!("对话协议无效");
    }
    settings.max_file_mb = settings.max_file_mb.clamp(1, 32);
    settings.min_free_disk_gb = settings.min_free_disk_gb.clamp(2, 100);
    settings.max_index_gb = settings.max_index_gb.clamp(1, 1024);
    settings.auto_local_chunk_limit = settings.auto_local_chunk_limit.clamp(1000, 10_000_000);
    settings.api_embedding_batch_size = settings.api_embedding_batch_size.clamp(1, 128);
    if !["low", "balanced", "performance"].contains(&settings.indexing_profile.as_str()) {
        bail!("索引资源占用选项无效");
    }
    settings.reconcile_seconds = settings.reconcile_seconds.clamp(30, 86400);
    settings.file_quiet_seconds=settings.file_quiet_seconds.min(600);
    for (kind, provider) in [
        ("chat", &mut settings.chat),
        ("embedding", &mut settings.embedding),
        ("rerank", &mut settings.rerank),
    ] {
        if let Some(value) = secrets[kind].as_str() {
            if !value.is_empty() {
                let credential = format!("{kind}:{}", identifier());
                app.models.save_key(&credential, value)?;
                provider.credential_id = Some(credential);
            } else {
                provider.credential_id = None;
            }
            provider.has_key = !value.is_empty();
        }
    }
    {
        let mut db = app.store.connection.lock().unwrap();
        let transaction = db.transaction()?;
        if !changes.contains_key("onboarding_complete") {
            // A guide can finish while provider validation/keychain work is in
            // progress. Preserve its most recent flag under the writer lock.
            settings.onboarding_complete = transaction.query_row(
                "SELECT COALESCE(json_extract(value,'$.onboarding_complete'),0) FROM metadata WHERE key='settings'",
                [], |row| row.get::<_, bool>(0),
            ).optional()?.unwrap_or(false);
        }
        transaction.execute("INSERT INTO metadata(key,value) VALUES('settings',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [serde_json::to_string(&settings)?])?;
        transaction.commit()?;
    }
    for (kind, previous, current) in [
        ("chat", &old.chat, &settings.chat),
        ("embedding", &old.embedding, &settings.embedding),
        ("rerank", &old.rerank, &settings.rerank),
    ] {
        if previous.has_key && (previous.credential_id != current.credential_id || !current.has_key)
        {
            let _ = app
                .models
                .save_key(previous.credential_id.as_deref().unwrap_or(kind), "");
        }
    }
    app.wake.notify_one();
    Ok(Json(json!(settings)))
}

async fn test_model(State(state): State<ApiState>, Path(kind): Path<String>) -> ApiResult {
    Ok(Json(
        state
            .app
            .models
            .test(&state.app.store.settings()?, &kind)
            .await?,
    ))
}
async fn create_connection(State(state): State<ApiState>, Json(input): Json<Value>) -> ApiResult {
    let _maintenance = state.app.maintenance.read().await;
    Ok(Json(connections::create(&state.app, input)?))
}
async fn update_connection(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(input): Json<Value>,
) -> ApiResult {
    let _maintenance = state.app.maintenance.read().await;
    Ok(Json(json!(connections::update(&state.app, &id, input)?)))
}
async fn connection_config(State(state): State<ApiState>, Path(id): Path<String>) -> ApiResult {
    Ok(Json(connections::config(&state.app, &id)?))
}
async fn install_zcode(State(state): State<ApiState>, Path(id): Path<String>) -> ApiResult {
    let _maintenance = state.app.maintenance.read().await;
    Ok(Json(connections::install_zcode(&state.app, &id)?))
}
async fn test_connection(State(state): State<ApiState>, Path(id): Path<String>) -> ApiResult {
    let app = &state.app;
    let client = app
        .store
        .authenticate(&id, &connections::token(app, &id)?)?;
    let scope = Scope::Agent(client);
    let projects = retrieval::projects(app, &scope)?;
    let request = retrieval::SearchRequest {
        query: "项目 技术 方案 实验".into(),
        project_id: None,
        collection_id: None,
        limit: Some(3),
        extension: None,
        updated_after: None,
    };
    let context = retrieval::context(app, &scope, &request).await?;
    Ok(Json(
        json!({"ok":true,"projects":projects.len(),"sources":context["sources"].as_array().map(Vec::len).unwrap_or_default(),"message":"知识库检索与当前共享权限测试通过。请在 ZCode 新会话中完成实际调用验证。"}),
    ))
}

async fn export_backup(State(state): State<ApiState>) -> std::result::Result<Response, ApiError> {
    let bytes = backup::export(&state.app).await?;
    Ok((
        [
            ("content-type", "application/zip"),
            (
                "content-disposition",
                "attachment; filename=personal-knowledge.pkb",
            ),
        ],
        bytes,
    )
        .into_response())
}
async fn restore_backup(State(state): State<ApiState>, bytes: Bytes) -> ApiResult {
    Ok(Json(backup::restore(&state.app, bytes.to_vec()).await?))
}

async fn shutdown(State(state): State<ApiState>) -> Json<Value> {
    state.app.stop();
    Json(json!({"ok":true}))
}

#[derive(Deserialize)]
pub struct ToolBody {
    #[serde(default)]
    pub arguments: Value,
}

pub async fn invoke_tool(
    app: &Arc<AppState>,
    client: Connection,
    tool: &str,
    input: Value,
) -> Result<Value> {
    crate::queries::run(app.clone(),Scope::Agent(client),tool.into(),input,crate::queries::RequestLife::new()).await
}

async fn agent_tool(
    State(state): State<ApiState>,
    Path((id, tool)): Path<(String, String)>,
    headers: HeaderMap,
    Extension(life):Extension<Arc<crate::queries::RequestLife>>,
    Json(input): Json<ToolBody>,
) -> std::result::Result<Json<Value>, ApiError> {
    if !origin_allowed(&headers) {
        bail!("来源不可用");
    }
    let token = headers
        .get("authorization")
        .and_then(|header| header.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .context("连接未授权")?;
    let client = state.app.store.authenticate(&id, token)?;
    Ok(Json(
        crate::queries::run(state.app.clone(),Scope::Agent(client),tool,input.arguments,life).await?,
    ))
}

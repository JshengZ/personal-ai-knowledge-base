use crate::{backup, connections, indexer, knowledge, retrieval, state::AppState, types::*};
use anyhow::{Context, Result};
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use serde::Deserialize;
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
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":self.0.to_string()})),
        )
            .into_response()
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

pub fn router(app: Arc<AppState>, token: String) -> Router {
    let state = ApiState { app, token };
    let owner = Router::new()
        .route("/health", get(health))
        .route("/state", get(snapshot))
        .route("/documents", get(documents))
        .route("/roots", post(add_root))
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
        .route("/models/test/{kind}", post(test_model))
        .route("/connections", post(create_connection))
        .route("/connections/{id}", patch(update_connection))
        .route("/connections/{id}/config", get(connection_config))
        .route("/connections/{id}/install-zcode", post(install_zcode))
        .route("/connections/{id}/test", post(test_connection))
        .route("/backups/export", get(export_backup))
        .route("/backups/restore", post(restore_backup))
        .route("/demo", post(demo))
        .route("/shutdown", post(shutdown))
        .layer(middleware::from_fn_with_state(state.clone(), owner_auth))
        .layer(DefaultBodyLimit::max(512 * 1024 * 1024));
    Router::new()
        .nest("/api", owner)
        .route("/agent/{id}/{tool}", post(agent_tool))
        .route("/mcp/{id}", axum::routing::any(crate::mcp::http_handler))
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({"ok":true,"version":env!("CARGO_PKG_VERSION")}))
}

async fn snapshot(State(state): State<ApiState>) -> ApiResult {
    let app = &state.app;
    let mut documents = app.store.documents()?;
    documents.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let settings = app.store.settings()?;
    let semantic_error = app.semantic_error.read().await.clone();
    Ok(Json(
        json!({"version":env!("CARGO_PKG_VERSION"),"roots":app.store.roots()?,"projects":app.store.projects()?,
        "documents":documents.into_iter().take(100).collect::<Vec<_>>(),"memories":app.store.memories()?,
        "connections":app.store.connections()?,"stats":app.store.stats()?,"settings":settings,
        "events":app.store.events(60)?,"last_indexed":app.store.metadata::<String>("last_indexed")?,
        "last_scan":app.store.metadata::<String>("last_scan")?,"scanning":app.scanning.load(Ordering::SeqCst),
        "embedding_busy":app.embedding_busy.load(Ordering::SeqCst),"embedding_available":app.models.embedding_available(&settings),
        "embedding_model":app.models.embedding_model(&settings),"semantic_error":semantic_error,"data_directory":app.home,
        "demo_created":app.store.metadata::<bool>("demo_created")?.unwrap_or(false)}),
    ))
}

async fn documents(
    State(state): State<ApiState>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult {
    let mut documents = state.app.store.documents()?;
    documents.retain(|document| {
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
    documents.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
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

fn add_root_impl(app: &AppState, input: &Value) -> Result<Root> {
    let raw = input["path"].as_str().context("请选择资料目录")?;
    let path = std::path::Path::new(raw)
        .canonicalize()
        .context("目录无法访问，请重新选择")?;
    if !path.is_dir() {
        bail!("请选择一个文件夹");
    }
    if path.starts_with(&app.home) || app.home.starts_with(&path) {
        bail!("知识库的数据目录已由应用管理，请选择原始资料目录");
    }
    let path = path.to_string_lossy().into_owned();
    if app.store.roots()?.iter().any(|root| {
        std::path::Path::new(&path).starts_with(&root.path)
            || std::path::Path::new(&root.path).starts_with(&path)
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
    let root = Root {
        id: identifier(),
        path,
        name,
        policy: policy.into(),
        enabled: true,
        exclusions,
        created_at: timestamp(),
        last_scan: None,
        error: None,
    };
    app.store.save_root(&root)?;
    app.store.ensure_project(&root.path, &root.name)?;
    app.wake.notify_one();
    Ok(root)
}

async fn add_root(State(state): State<ApiState>, Json(input): Json<Value>) -> ApiResult {
    let _maintenance = state.app.maintenance.read().await;
    Ok(Json(json!(add_root_impl(&state.app, &input)?)))
}

async fn update_root(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(input): Json<Value>,
) -> ApiResult {
    let app = &state.app;
    let _maintenance = app.maintenance.write().await;
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
                app.store.remove_document(&document.id)?;
            }
            root.path = canonical.to_string_lossy().into_owned();
            root.last_scan = None;
            root.error = None;
            app.store.ensure_project(&root.path, &root.name)?;
        }
    }
    if let Some(name) = input["name"].as_str() {
        if name.trim().is_empty() {
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
    app.store.save_root(&root)?;
    app.wake.notify_one();
    Ok(Json(json!(root)))
}

async fn remove_root(State(state): State<ApiState>, Path(id): Path<String>) -> ApiResult {
    let app = &state.app;
    let _maintenance = app.maintenance.write().await;
    for document in app
        .store
        .documents()?
        .into_iter()
        .filter(|document| document.root_id == id)
    {
        app.store.remove_document(&document.id)?;
    }
    {
        let db = app.store.connection.lock().unwrap();
        db.execute("DELETE FROM jobs WHERE root_id=?1", [&id])?;
        db.execute("DELETE FROM roots WHERE id=?1", [&id])?;
    }
    app.wake.notify_one();
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
        retrieval::search(&state.app, &Scope::Owner, &input).await?
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
    settings.max_file_mb = settings.max_file_mb.clamp(1, 100);
    settings.reconcile_seconds = settings.reconcile_seconds.clamp(30, 3600);
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
    app.store.set_metadata("settings", &settings)?;
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

async fn demo(State(state): State<ApiState>) -> ApiResult {
    let _maintenance = state.app.maintenance.read().await;
    let directory = state
        .app
        .home
        .with_file_name(format!(
            "个人知识库示例资料-{}",
            &blake3::hash(state.app.home.to_string_lossy().as_bytes()).to_hex()[..8]
        ))
        .join("推荐排序");
    std::fs::create_dir_all(&directory)?;
    let directory = directory.canonicalize()?;
    let experiment="# 排序实验\n\n这些内容是产品体验用的示例资料。\n\n## 项目目标\n比较排序模型的离线效果，验证上线方案。\n\n## 实验\n使用统一的验证集比较 LR 基线和 LightGBM。\nLightGBM 的 NDCG@10 为 0.462，LR 为 0.438。\n\n## 方案\n下一轮灰度采用 LightGBM LambdaRank，LR 作为对照基线。\n";
    std::fs::write(directory.join("排序实验.md"), experiment)?;
    std::fs::write(directory.join("train_lgbm.py"),"# 产品示例训练配置\nparams = {\n    'objective': 'lambdarank',\n    'metric': 'ndcg',\n    'ndcg_eval_at': [10],\n}\n# LightGBM LambdaRank；LR 用于基线对照。\n")?;
    std::fs::write(directory.join("上线决策.md"),"# 上线决策\n\n## 候选方案\n采用 LightGBM 进入灰度验证，保留 LR 作为对照基线。\n\n## 后续观察\n继续观测线上延迟和高峰负载。\n")?;
    let path = directory.to_string_lossy().into_owned();
    if !state
        .app
        .store
        .roots()?
        .iter()
        .any(|root| root.path == path)
    {
        add_root_impl(
            &state.app,
            &json!({"path":path,"name":"推荐排序 · 示例","policy":"agents"}),
        )?;
    }
    let mut project = state.app.store.ensure_project(&path, "推荐排序 · 示例")?;
    project.goal = "比较排序模型的效果，验证上线方案。".into();
    project.approach = "采用 LightGBM LambdaRank，保留 LR 作为对照基线。".into();
    project.decisions = vec![Decision {
        date: chrono::Local::now().format("%Y-%m-%d").to_string(),
        text: "示例项目进入灰度验证".into(),
    }];
    state.app.store.save_project(&project)?;
    state.app.store.set_metadata("demo_created", &true)?;
    Ok(Json(json!({"ok":true,"project_id":project.id})))
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
    let scope = Scope::Agent(client);
    match tool {
        "get_context" => retrieval::context(app, &scope, &serde_json::from_value(input)?).await,
        "search" => {
            let result = retrieval::search(app, &scope, &serde_json::from_value(input)?).await?;
            app.audit(&scope, "search", result.hits.len());
            Ok(serde_json::to_value(result)?)
        }
        "read_source" => retrieval::read_source(app, &scope, &serde_json::from_value(input)?),
        "list_projects" => {
            let projects = retrieval::projects(app, &scope)?;
            app.audit(&scope, "list_projects", projects.len());
            Ok(json!({"projects":projects}))
        }
        "propose_memory" => Ok(json!(retrieval::propose(
            app,
            &scope,
            serde_json::from_value(input)?
        )?)),
        _ => bail!("知识工具不存在"),
    }
}

async fn agent_tool(
    State(state): State<ApiState>,
    Path((id, tool)): Path<(String, String)>,
    headers: HeaderMap,
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
        invoke_tool(&state.app, client, &tool, input.arguments).await?,
    ))
}

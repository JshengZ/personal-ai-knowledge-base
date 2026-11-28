use crate::{
    diagnostics::{Audit, Diagnostics},
    errors::ServiceError,
    state::AppState,
    types::*,
};
use anyhow::Result;
use axum::{
    body::{Body, Bytes},
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use http_body::{Body as HttpBody, Frame, SizeHint};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

pub const QUERY_SECONDS: u64 = 8;
pub const BRIDGE_SECONDS: u64 = 15;
#[derive(Clone)]
pub struct QueryContext {
    pub cancel: CancellationToken,
    pub deadline: Instant,
}
impl QueryContext {
    pub fn expired(&self) -> bool {
        self.cancel.is_cancelled() || Instant::now() >= self.deadline
    }
}
thread_local! { static CONTEXT:RefCell<Option<QueryContext>>=const{RefCell::new(None)}; }
pub fn current() -> Option<QueryContext> {
    CONTEXT.with(|context| context.borrow().clone())
}
pub fn check() -> Result<()> {
    if current().is_some_and(|context| context.expired()) {
        return Err(ServiceError::new(
            "query_cancelled",
            "查询已取消或达到执行时限",
            StatusCode::REQUEST_TIMEOUT,
        )
        .into());
    }
    Ok(())
}
struct ContextGuard(Option<QueryContext>);
impl ContextGuard {
    fn set(value: QueryContext) -> Self {
        Self(CONTEXT.with(|slot| slot.replace(Some(value))))
    }
}
impl Drop for ContextGuard {
    fn drop(&mut self) {
        CONTEXT.with(|slot| slot.replace(self.0.take()));
    }
}

pub struct RequestLife {
    pub id: String,
    pub cancel: CancellationToken,
    started: Instant,
    event: Mutex<Option<Audit>>,
    diagnostics: Mutex<Option<Arc<Diagnostics>>>,
}
impl RequestLife {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            id: identifier(),
            cancel: CancellationToken::new(),
            started: Instant::now(),
            event: Mutex::new(None),
            diagnostics: Mutex::new(None),
        })
    }
    pub fn begin(&self, app: &AppState, scope: &Scope, tool: &str) {
        if let Scope::Agent(client) = scope {
            *self.diagnostics.lock().unwrap() = Some(app.diagnostics.clone());
            *self.event.lock().unwrap() = Some(Audit {
                client_id: client.id.clone(),
                tool: tool.into(),
                request_id: self.id.clone(),
                phase: "started",
                count: 0,
                elapsed_ms: 0,
                mode: None,
                error_code: None,
            });
        }
    }
    pub fn record(
        &self,
        phase: &'static str,
        result: Option<&Value>,
        error: Option<&anyhow::Error>,
    ) {
        let mut event = self.event.lock().unwrap();
        let Some(event) = event.as_mut() else { return };
        event.phase = phase;
        event.elapsed_ms = self.started.elapsed().as_millis();
        if let Some(value) = result {
            event.count = value["hits"]
                .as_array()
                .or(value["sources"].as_array())
                .or(value["projects"].as_array())
                .or(value["chunks"].as_array())
                .map(Vec::len)
                .unwrap_or(1);
            event.mode = value["mode"].as_str().map(str::to_string);
        }
        if let Some(error) = error {
            event.error_code = Some(crate::errors::describe(error).code.into());
        }
        if let Some(diagnostics) = self.diagnostics.lock().unwrap().as_ref() {
            diagnostics.audit(event.clone());
        }
    }
}
struct CancelOnDrop(Option<Arc<RequestLife>>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(life) = self.0.take() {
            life.cancel.cancel();
            life.record("transport_interrupted", None, None);
        }
    }
}
struct ResponseBody {
    inner: Body,
    life: Arc<RequestLife>,
    complete: bool,
}
impl ResponseBody {
    fn complete(&mut self) {
        if !self.complete {
            self.complete = true;
            self.life.record("transport_complete", None, None);
        }
    }
}
impl HttpBody for ResponseBody {
    type Data = Bytes;
    type Error = axum::Error;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<std::result::Result<Frame<Bytes>, axum::Error>>> {
        let result = Pin::new(&mut self.inner).poll_frame(cx);
        if matches!(result, Poll::Ready(None))
            || matches!(result, Poll::Ready(Some(Ok(_)))) && self.inner.is_end_stream()
        {
            self.complete();
        }
        result
    }
    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }
    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}
impl Drop for ResponseBody {
    fn drop(&mut self) {
        if !self.complete {
            self.life.cancel.cancel();
            self.life.record("transport_interrupted", None, None);
        }
    }
}
pub async fn track(mut request: Request, next: Next) -> Response {
    if !request.uri().path().starts_with("/agent/") && !request.uri().path().starts_with("/mcp/") {
        return next.run(request).await;
    }
    let life = RequestLife::new();
    request.extensions_mut().insert(life.clone());
    let mut guard = CancelOnDrop(Some(life.clone()));
    let response = next.run(request).await;
    guard.0.take();
    let (mut parts, body) = response.into_parts();
    parts
        .headers
        .insert("x-pkb-request-id", life.id.parse().unwrap());
    let complete = body.is_end_stream();
    if complete {
        life.record("transport_complete", None, None);
    }
    Response::from_parts(
        parts,
        Body::new(ResponseBody {
            inner: body,
            life,
            complete,
        }),
    )
}

pub async fn run(
    app: Arc<AppState>,
    scope: Scope,
    tool: String,
    input: Value,
    life: Arc<RequestLife>,
) -> Result<Value> {
    life.begin(&app, &scope, &tool);
    let write = tool == "propose_memory";
    let capacity = || {
        anyhow::Error::from(ServiceError::new("query_capacity","查询通道已满，请稍后重试",StatusCode::SERVICE_UNAVAILABLE).details(json!({"normal_active":6-app.query_lanes.available_permits(),"keyword_active":2-app.keyword_lanes.available_permits()})))
    };
    let permits = match app.query_lanes.clone().try_acquire_owned() {
        Ok(permit) => Ok((permit, false)),
        Err(_) if matches!(tool.as_str(), "search" | "get_context" | "owner_search") => app
            .keyword_lanes
            .clone()
            .try_acquire_owned()
            .map(|permit| (permit, true))
            .map_err(|_| capacity()),
        Err(_) => Err(capacity()),
    };
    let (permit, keyword) = match permits {
        Ok(value) => value,
        Err(error) => {
            life.record("failed", None, Some(&error));
            return Err(error);
        }
    };
    let timeout = if write { 120 } else { QUERY_SECONDS };
    let cancel = life.cancel.clone();
    let context = QueryContext {
        cancel: cancel.clone(),
        deadline: Instant::now() + Duration::from_secs(timeout),
    };
    let handle = tokio::runtime::Handle::current();
    let worker = app.clone();
    let operation = tool.clone();
    let work = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let _context = ContextGuard::set(context);
        let _task = worker
            .diagnostics
            .task("query", json!({"tool":operation,"keyword_only":keyword}));
        handle.block_on(async {
            let _maintenance = worker.maintenance.try_read().map_err(|_| {
                ServiceError::new(
                    "maintenance_busy",
                    "知识库正在备份、恢复或迁移，请稍后重试",
                    StatusCode::SERVICE_UNAVAILABLE,
                )
            })?;
            check()?;
            match operation.as_str() {
                "search" | "owner_search" => Ok(serde_json::to_value(
                    crate::retrieval::search_mode(
                        &worker,
                        &scope,
                        &serde_json::from_value(input)?,
                        keyword,
                    )
                    .await?,
                )?),
                "get_context" => {
                    crate::retrieval::context_mode(
                        &worker,
                        &scope,
                        &serde_json::from_value(input)?,
                        keyword,
                    )
                    .await
                }
                "list_projects" => {
                    Ok(json!({"projects":crate::retrieval::projects(&worker,&scope)?}))
                }
                "read_source" => {
                    crate::retrieval::read_source(&worker, &scope, &serde_json::from_value(input)?)
                }
                "propose_memory" => {
                    let _library = worker.library_lock.lock().unwrap();
                    check()?;
                    Ok(json!(crate::retrieval::propose(
                        &worker,
                        &scope,
                        serde_json::from_value(input)?
                    )?))
                }
                _ => {
                    Err(
                        ServiceError::new("unknown_tool", "知识工具不存在", StatusCode::NOT_FOUND)
                            .into(),
                    )
                }
            }
        })
    });
    let result = tokio::select! {
        result=work=>result.map_err(anyhow::Error::from).and_then(|value|value),
        _=life.cancel.cancelled()=>Err(ServiceError::new("request_cancelled","请求方已取消；已提交的写入请按结果状态核对",StatusCode::REQUEST_TIMEOUT).into()),
        _=app.shutdown.cancelled()=>Err(ServiceError::new("service_stopping","知识服务正在退出",StatusCode::SERVICE_UNAVAILABLE).into()),
        _=tokio::time::sleep(Duration::from_secs(timeout))=>Err(ServiceError::new(if write {"mutation_outcome_unknown"}else{"query_timeout"},if write {"提交结果暂未确认；相同记忆建议重试会返回已有待确认记录"}else{"查询达到执行时限，请缩小检索范围后重试"},StatusCode::GATEWAY_TIMEOUT).details(json!({"deadline_seconds":timeout,"request_id":life.id})).into()),
    };
    let phase = if result.is_ok() {
        "computed"
    } else if cancel.is_cancelled() {
        "cancelled"
    } else {
        "failed"
    };
    if result.is_err() {
        cancel.cancel();
    }
    life.record(phase, result.as_ref().ok(), result.as_ref().err());
    result
}

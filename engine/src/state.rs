use crate::{models::Models, store::Store, types::*};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::{Notify, RwLock};

pub struct AppState {
    pub store: Arc<Store>,
    pub models: Arc<Models>,
    pub parser: crate::parser::ParserService,
    pub runtime: RuntimeConfig,
    pub home: PathBuf,
    pub wake: Notify,
    pub maintenance: RwLock<()>,
    pub stopping: AtomicBool,
    pub scanning: AtomicBool,
    pub embedding_busy: AtomicBool,
    pub semantic_error: RwLock<Option<String>>,
}

impl AppState {
    pub fn new(home: PathBuf, runtime: RuntimeConfig) -> Result<Arc<Self>> {
        std::fs::create_dir_all(&home)?;
        let home = home.canonicalize()?;
        let store = Arc::new(Store::open(&home.join("knowledge.sqlite"))?);
        if store.metadata::<Value>("settings")?.is_none() {
            let mut settings = Settings::default();
            if let Ok(bytes) = std::fs::read(runtime.resources.join("company-config.json")) {
                if let Ok(config) = serde_json::from_slice::<Value>(&bytes) {
                    settings.organization =
                        config["organization"].as_str().unwrap_or_default().into();
                    for (name, provider) in [
                        ("chat", &mut settings.chat),
                        ("embedding", &mut settings.embedding),
                        ("rerank", &mut settings.rerank),
                    ] {
                        if let Some(value) = config.get(name) {
                            provider.base_url =
                                value["base_url"].as_str().unwrap_or_default().into();
                            provider.model = value["model"].as_str().unwrap_or_default().into();
                            provider.protocol =
                                value["protocol"].as_str().unwrap_or("openai").into();
                            provider.endpoint =
                                value["endpoint"].as_str().unwrap_or_default().into();
                        }
                    }
                    settings.embedding_mode = config["embedding"]["mode"]
                        .as_str()
                        .filter(|mode| ["auto", "local", "disabled"].contains(mode))
                        .unwrap_or("auto")
                        .into();
                    settings.rerank_enabled =
                        config["rerank"]["enabled"].as_bool().unwrap_or(false);
                }
            }
            store.set_metadata("settings", &settings)?;
        }
        let models = Models::new(&home, runtime.clone())?;
        Ok(Arc::new(Self {
            store,
            models,
            parser: crate::parser::ParserService::new(runtime.clone()),
            runtime,
            home,
            wake: Notify::new(),
            maintenance: RwLock::new(()),
            stopping: AtomicBool::new(false),
            scanning: AtomicBool::new(false),
            embedding_busy: AtomicBool::new(false),
            semantic_error: RwLock::new(None),
        }))
    }

    pub fn allowed(
        &self,
        scope: &Scope,
        project_id: Option<&str>,
        company_only: bool,
    ) -> Result<(Vec<String>, Vec<String>)> {
        let current = match scope {
            Scope::Owner => None,
            Scope::Agent(client) => Some(self.store.client(&client.id)?),
        };
        if current.as_ref().is_some_and(|client| !client.enabled) {
            return Ok((vec![], vec![]));
        }
        let eligible_roots: Vec<_> = self
            .store
            .roots()?
            .into_iter()
            .filter(|root| {
                root.enabled
                    && (!company_only || root.policy != "local")
                    && (matches!(scope, Scope::Owner) || root.policy == "agents")
            })
            .collect();
        let projects = self
            .store
            .projects()?
            .into_iter()
            .filter(|project| {
                eligible_roots
                    .iter()
                    .any(|root| Path::new(&project.root_path).starts_with(&root.path))
                    && project_id.is_none_or(|id| project.id == id)
                    && match scope {
                        Scope::Owner => true,
                        Scope::Agent(_) => current
                            .as_ref()
                            .is_some_and(|client| client.project_ids.contains(&project.id)),
                    }
            })
            .map(|project| project.id)
            .collect();
        let roots = eligible_roots.into_iter().map(|root| root.id).collect();
        Ok((roots, projects))
    }

    pub fn check_document(
        &self,
        scope: &Scope,
        document: &Document,
        company_only: bool,
    ) -> Result<()> {
        let (roots, projects) = self.allowed(scope, None, company_only)?;
        if !roots.contains(&document.root_id) || !projects.contains(&document.project_id) {
            bail!("当前连接无权读取这份资料");
        }
        let root = self.store.root(&document.root_id)?;
        let path = Path::new(&document.path)
            .canonicalize()
            .context("原文件已移走或暂不可用")?;
        let root_path = Path::new(&root.path)
            .canonicalize()
            .context("资料目录暂不可用")?;
        if !path.starts_with(root_path) {
            bail!("原文件已经离开授权目录");
        }
        let metadata = std::fs::metadata(&path)?;
        let modified = modified_ms(&metadata);
        if metadata.len() != document.size || modified != document.modified_ms {
            let _ = self.store.enqueue(&document.path, &document.root_id, true);
            self.wake.notify_one();
            bail!("原文件正在更新，请等索引完成后重试");
        }
        if document.status != "ready" {
            bail!("这份资料尚未完成索引");
        }
        Ok(())
    }

    pub fn valid_evidence(&self, scope: &Scope, evidence: &[Evidence], company_only: bool) -> bool {
        evidence.iter().all(|item| {
            self.store
                .document(&item.document_id)
                .is_ok_and(|document| {
                    document.version == item.version
                        && self.check_document(scope, &document, company_only).is_ok()
                })
        })
    }

    pub fn visible_memories(&self, scope: &Scope, project_id: Option<&str>) -> Result<Vec<Memory>> {
        if let Scope::Agent(client) = scope {
            let current = self.store.client(&client.id)?;
            if !current.enabled || !current.include_profile {
                return Ok(vec![]);
            }
        }
        let (_, projects) = self.allowed(scope, project_id, true)?;
        Ok(self
            .store
            .memories()?
            .into_iter()
            .filter(|memory| {
                memory.status == "confirmed"
                    && memory
                        .project_id
                        .as_ref()
                        .is_none_or(|id| projects.contains(id))
                    && self.valid_evidence(scope, &memory.evidence, true)
            })
            .collect())
    }

    pub fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        self.wake.notify_waiters();
    }

    pub fn audit(&self, scope: &Scope, tool: &str, count: usize) {
        if let Scope::Agent(client) = scope {
            let _ = self.store.touch_client(&client.id);
            let _ = self.store.event(
                "mcp",
                "Agent 调用了知识库",
                json!({"connection_id":client.id,"name":client.name,"tool":tool,"results":count}),
            );
        }
    }
}

pub fn modified_ms(metadata: &std::fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|time| time.as_millis() as u64)
        .unwrap_or_default()
}

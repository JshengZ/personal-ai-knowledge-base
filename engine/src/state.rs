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
    pub diagnostics: Arc<crate::diagnostics::Diagnostics>,
    pub query_lanes: Arc<tokio::sync::Semaphore>,
    pub keyword_lanes: Arc<tokio::sync::Semaphore>,
    pub models: Arc<Models>,
    pub parser: crate::parser::ParserService,
    pub runtime: RuntimeConfig,
    pub home: PathBuf,
    pub wake: Notify,
    pub maintenance: RwLock<()>,
    pub stopping: Arc<AtomicBool>,
    pub scanning: AtomicBool,
    pub embedding_busy: AtomicBool,
    pub semantic_error: RwLock<Option<String>>,
    pub requests: Arc<tokio::sync::Semaphore>,
    pub read_requests: Arc<tokio::sync::Semaphore>,
    pub library_lock: std::sync::Mutex<()>,
    pub shutdown: tokio_util::sync::CancellationToken,
    pub embedding_metrics: std::sync::Mutex<crate::progress::EmbeddingMetrics>,
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
        let mut settings = store.settings()?;
        if settings.migrate_policy() {store.set_metadata("settings",&settings)?;}
        if settings.max_file_mb > 32 {
            settings.max_file_mb = 32;
            store.set_metadata("settings", &settings)?;
        }
        let namespace = store
            .metadata::<String>("credential_namespace")?
            .unwrap_or_else(|| crate::storage::credential_namespace(&home));
        store.set_metadata("credential_namespace", &namespace)?;
        let temporary = home.join("tmp");
        std::fs::create_dir_all(&temporary)?;
        let models = Models::new(namespace, runtime.clone(), temporary.clone())?;
        Ok(Arc::new(Self {
            stopping: store.cancelled.clone(),
            diagnostics: crate::diagnostics::Diagnostics::new(&store),
            query_lanes: Arc::new(tokio::sync::Semaphore::new(6)),
            keyword_lanes: Arc::new(tokio::sync::Semaphore::new(2)),
            store,
            models,
            parser: crate::parser::ParserService::new(runtime.clone(), temporary),
            runtime,
            home,
            wake: Notify::new(),
            maintenance: RwLock::new(()),
            scanning: AtomicBool::new(false),
            embedding_busy: AtomicBool::new(false),
            semantic_error: RwLock::new(None),
            requests: Arc::new(tokio::sync::Semaphore::new(8)),
            read_requests: Arc::new(tokio::sync::Semaphore::new(4)),
            library_lock: std::sync::Mutex::new(()),
            shutdown: tokio_util::sync::CancellationToken::new(),
            embedding_metrics: std::sync::Mutex::new(crate::progress::EmbeddingMetrics::default()),
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
                    .any(|root| project.root_id.as_deref() == Some(root.id.as_str()))
                    && project_id.is_none_or(|id| project.id == id)
                    && match scope {
                        Scope::Owner => true,
                        Scope::Agent(_) => current
                            .as_ref()
                            .is_some_and(|client| client.scope_mode == "all" || client.project_ids.contains(&project.id)),
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
        if !crate::indexer::eligible(std::path::Path::new(&document.path), &root, self)? {
            bail!("这份资料已被排除，不再允许检索或读取");
        }
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
            let mut hints=self.diagnostics.hints.lock().unwrap();
            if hints.len()<4096 {hints.insert(std::path::PathBuf::from(&document.path));}
            drop(hints);
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
                        && item
                            .chunk_id
                            .as_ref()
                            .is_none_or(|id| self.store.has_chunk(&document.id, id))
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
        self.shutdown.cancel();
        self.models.request_stop();
        self.parser.request_stop();
        self.wake.notify_waiters();
    }

    pub fn indexing_allowed(&self) -> bool {
        self.reconciliation_allowed()
            && self.store.settings().is_ok_and(|settings| {
                crate::storage::require_capacity(&self.home, &settings, 64 * 1024 * 1024).is_ok()
            })
    }

    /// Deletions and exclusions must still reconcile when growth is blocked.
    pub fn reconciliation_allowed(&self) -> bool {
        self.cleanup_ready() && self.store.settings().is_ok_and(|settings| !settings.paused)
    }

    /// Pausing discovery does not cancel already authorized removals. A paused
    /// interrupted migration is kept unchanged so its ready copy can be reused.
    pub fn cleanup_allowed(&self) -> bool {
        self.cleanup_ready()
            && self.store.settings().is_ok_and(|settings| {
                !settings.paused || !crate::storage::migration_pending(&self.home)
            })
    }

    fn cleanup_ready(&self) -> bool {
        if self.stopping.load(Ordering::SeqCst) {
            return false;
        }
        self.store
            .counts()
            .is_ok_and(|counts| counts.get("initialized") == Some(&1))
    }

    pub fn audit(&self, scope: &Scope, tool: &str, count: usize) {
        if let Scope::Agent(client) = scope {
            self.diagnostics.audit(crate::diagnostics::Audit{client_id:client.id.clone(),tool:tool.into(),request_id:identifier(),phase:"computed",count,elapsed_ms:0,mode:None,error_code:None});
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

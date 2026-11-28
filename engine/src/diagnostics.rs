use crate::{store::Store, types::*};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Audit {
    pub client_id: String,
    pub tool: String,
    pub request_id: String,
    pub phase: &'static str,
    pub count: usize,
    pub elapsed_ms: u128,
    pub mode: Option<String>,
    pub error_code: Option<String>,
}
pub struct Diagnostics {
    started: Instant,
    pub active: Mutex<HashMap<String, Value>>,
    pub watchers: Mutex<HashMap<String, Value>>,
    pub changed: Mutex<HashMap<String, Vec<Instant>>>,
    pub watcher_drops: AtomicU64,
    pub hints: Mutex<std::collections::HashSet<std::path::PathBuf>>,
    pub audit_drops: Arc<AtomicU64>,
    audit: mpsc::SyncSender<Audit>,
}
impl Diagnostics {
    pub fn new(store: &Arc<Store>) -> Arc<Self> {
        let (send, receive) = mpsc::sync_channel::<Audit>(1024);
        let weak = Arc::downgrade(store);
        let dropped = Arc::new(AtomicU64::new(0));
        let counter = dropped.clone();
        std::thread::Builder::new()
            .name("pkb-audit".into())
            .spawn(move || {
                while weak.strong_count() > 0 {
                    let event = match receive.recv_timeout(Duration::from_millis(100)) {
                        Ok(event) => event,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(_) => break,
                    };
                    let until = Instant::now() + Duration::from_secs(60);
                    loop {
                        let Some(store) = weak.upgrade() else {
                            return;
                        };
                        if store.cancelled.load(Ordering::SeqCst) {
                            return;
                        }
                        let written = store.try_audit(&event).unwrap_or(false);
                        drop(store);
                        if written {
                            break;
                        }
                        if Instant::now() >= until {
                            counter.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
            })
            .expect("audit worker");
        Arc::new(Self {
            started: Instant::now(),
            active: Mutex::new(HashMap::new()),
            watchers: Mutex::new(HashMap::new()),
            changed: Mutex::new(HashMap::new()),
            watcher_drops: AtomicU64::new(0),
            hints: Mutex::new(std::collections::HashSet::new()),
            audit_drops: dropped,
            audit: send,
        })
    }
    pub fn audit(&self, event: Audit) {
        if self.audit.try_send(event).is_err() {
            self.audit_drops.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub fn task(self: &Arc<Self>, kind: &str, details: Value) -> Task {
        let id = identifier();
        self.active.lock().unwrap().insert(
            id.clone(),
            json!({"kind":kind,"started_at":timestamp(),"details":details}),
        );
        Task {
            diagnostics: self.clone(),
            id,
        }
    }
    pub fn change(&self, root_id: &str) {
        let mut changed = self.changed.lock().unwrap();
        let list = changed.entry(root_id.into()).or_default();
        list.retain(|at| at.elapsed() < Duration::from_secs(600));
        if list.len() < 10_000 {
            list.push(Instant::now());
        }
    }
    pub fn snapshot(&self) -> Value {
        let mut changed = self.changed.lock().unwrap();
        changed.retain(|_, events| {
            events.retain(|at| at.elapsed() < Duration::from_secs(600));
            !events.is_empty()
        });
        json!({"pid":std::process::id(),"role":"service","version":env!("CARGO_PKG_VERSION"),"uptime_seconds":self.started.elapsed().as_secs(),
            "active_tasks":self.active.lock().unwrap().values().cloned().collect::<Vec<_>>(),"watchers":*self.watchers.lock().unwrap(),
            "source_activity":changed.iter().map(|(id,list)|json!({"root_id":id,"changes_10m":list.len(),"high_activity":list.len()>=100})).collect::<Vec<_>>(),
            "watcher_events_dropped":self.watcher_drops.load(Ordering::Relaxed),"audit_events_dropped":self.audit_drops.load(Ordering::Relaxed)})
    }
}
pub struct Task {
    diagnostics: Arc<Diagnostics>,
    id: String,
}
impl Drop for Task {
    fn drop(&mut self) {
        self.diagnostics.active.lock().unwrap().remove(&self.id);
    }
}

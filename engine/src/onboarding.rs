use crate::{store::Store, types::Settings};
use anyhow::{bail, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    pub version: u8,
    pub status: String,
}

impl Default for Progress {
    fn default() -> Self {
        Self { version: 1, status: "new".into() }
    }
}

pub fn load(store: &Store) -> Result<Progress> {
    let value: Progress = store.metadata("onboarding")?.unwrap_or_default();
    if value.version != 1 || !["new", "skipped", "completed"].contains(&value.status.as_str()) {
        bail!("使用指南记录格式无效");
    }
    Ok(value)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Update {
    pub status: String,
}

// Complete the optional introduction atomically, without saving any model,
// credential or folder draft from the renderer. Read settings under the writer
// transaction so concurrent settings saves are not overwritten by a stale copy.
pub fn update(store: &Store, input: Update) -> Result<Progress> {
    if !["skipped", "completed"].contains(&input.status.as_str()) {
        bail!("请选择跳过或完成使用指南");
    }
    let value = Progress { version: 1, status: input.status };
    let mut db = store.connection.lock().unwrap();
    let transaction = db.transaction()?;
    let raw: Option<String> = transaction.query_row("SELECT value FROM metadata WHERE key='settings'", [], |row| row.get(0)).optional()?;
    let mut settings: Settings = raw.map(|raw| serde_json::from_str(&raw)).transpose()?.unwrap_or_default();
    settings.onboarding_complete = true;
    for (key, data) in [("settings", serde_json::to_string(&settings)?), ("onboarding", serde_json::to_string(&value)?)] {
        transaction.execute("INSERT INTO metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, data])?;
    }
    transaction.commit()?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guide_progress_is_persistent_and_changes_only_its_own_settings_flag() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("knowledge.sqlite");
        let store = Store::open(&path).unwrap();
        assert_eq!(load(&store).unwrap(), Progress::default());
        let mut settings = store.settings().unwrap();
        settings.chat.model = "existing-model".into();
        settings.embedding_mode = "disabled".into();
        settings.paused = true;
        store.set_metadata("settings", &settings).unwrap();
        update(&store, Update { status: "skipped".into() }).unwrap();
        settings.onboarding_complete = true;
        assert_eq!(serde_json::to_value(store.settings().unwrap()).unwrap(), serde_json::to_value(settings).unwrap());
        assert!(update(&store, Update { status: "new".into() }).is_err());
        assert!(serde_json::from_value::<Update>(serde_json::json!({"status":"completed","chat":{}})).is_err());
        update(&store, Update { status: "completed".into() }).unwrap();
        assert!(store.roots().unwrap().is_empty());
        assert!(store.projects().unwrap().is_empty());
        assert!(store.memories().unwrap().is_empty());
        assert!(store.connections().unwrap().is_empty());
        drop(store);
        assert_eq!(load(&Store::open(&path).unwrap()).unwrap().status, "completed");
    }
}

use crate::{connections::private_write, state::AppState, types::*};
use anyhow::{bail, Context, Result};
use rusqlite::{backup::Backup, Connection as Sqlite, OptionalExtension};
use serde_json::json;
use std::{
    io::{Cursor, Read, Write},
    sync::Arc,
    time::Duration,
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        for suffix in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}

fn room_for_temporary(state: &AppState, bytes: u64) -> Result<()> {
    let settings = state.store.settings()?;
    let reserve = settings.min_free_disk_gb.clamp(2, 100) * crate::storage::GIB;
    if fs2::available_space(&state.home)? < reserve.saturating_add(bytes) {
        bail!("存储盘空间不足，无法安全创建备份或恢复临时文件，请先迁移数据或释放空间");
    }
    Ok(())
}

pub async fn export(state: &Arc<AppState>) -> Result<Vec<u8>> {
    let _maintenance = state.maintenance.write().await;
    export_locked(state)
}

fn export_locked(state: &AppState) -> Result<Vec<u8>> {
    let size = std::fs::metadata(state.home.join("knowledge.sqlite"))?.len();
    let wal = std::fs::metadata(state.home.join("knowledge.sqlite-wal"))
        .map(|m| m.len())
        .unwrap_or(0);
    if size.saturating_add(wal) > crate::storage::GIB {
        bail!("当前备份格式支持最多 1 GB 数据库，较大的知识库请使用存储迁移功能");
    }
    room_for_temporary(
        state,
        size.saturating_add(wal).saturating_add(64 * 1024 * 1024),
    )?;
    let temporary = state.home.join(format!("backup-{}.sqlite", identifier()));
    let _cleanup = Cleanup(temporary.clone());
    {
        let source = state.store.connection.lock().unwrap();
        let mut target = Sqlite::open(&temporary)?;
        let backup = Backup::new(&source, &mut target)?;
        backup.run_to_completion(100, Duration::from_millis(10), None)?;
    }
    {
        let snapshot = Sqlite::open(&temporary)?;
        snapshot.execute_batch("DELETE FROM connections; DELETE FROM jobs; DELETE FROM events;")?;
        let mut settings = state.store.settings()?;
        settings.chat.has_key = false;
        settings.embedding.has_key = false;
        settings.rerank.has_key = false;
        settings.chat.credential_id = None;
        settings.embedding.credential_id = None;
        settings.rerank.credential_id = None;
        settings.paused = true;
        snapshot.execute(
            "UPDATE metadata SET value=?1 WHERE key='settings'",
            [serde_json::to_string(&settings)?],
        )?;
        snapshot.execute(
            "DELETE FROM metadata WHERE key IN ('http_port','demo_created','credential_namespace')",
            [],
        )?;
    }
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("manifest.json", options)?;
    zip.write_all(&serde_json::to_vec_pretty(&json!({"format":"personal-knowledge-backup","schema":1,"created_at":timestamp(),"app_version":env!("CARGO_PKG_VERSION")}))?)?;
    zip.start_file("knowledge.sqlite", options)?;
    std::io::copy(&mut std::fs::File::open(&temporary)?, &mut zip)?;
    let bytes = zip.finish()?.into_inner();
    if bytes.len() > 512 * 1024 * 1024 {
        bail!("压缩备份超过 512 MB 上限，请使用存储迁移功能");
    }
    let _ = std::fs::remove_file(&temporary);
    Ok(bytes)
}


fn validate_records<T: serde::de::DeserializeOwned>(source: &Sqlite, table: &str) -> Result<()> {
    let mut statement = source.prepare(&format!("SELECT id,data FROM {table}"))?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let value: serde_json::Value = serde_json::from_str(&row.get::<_, String>(1)?)
            .with_context(|| format!("备份中的 {table} 数据无效"))?;
        if value["id"].as_str() != Some(id.as_str()) {
            bail!("备份中的 {table} 标识不一致");
        }
        let _: T = serde_json::from_value(value)
            .with_context(|| format!("备份中的 {table} 记录格式无效"))?;
    }
    Ok(())
}

// Never install arbitrary SQLite schema/triggers into the live library. Validate
// the input and copy permitted data into a fresh canonical Store first.
fn normalize_restore(source: &Sqlite, source_path: &std::path::Path, target_path: &std::path::Path, namespace: &str) -> Result<()> {
    for (table, columns) in [
        ("metadata", "key,value"), ("roots", "id,path,data"),
        ("projects", "id,path,data"),
        ("documents", "id,path,root_id,project_id,version,status,data"),
        ("chunks", "id,document_id,ordinal,text,location,lexical"),
        ("embeddings", "chunk_id,model,dimensions,vector"),
        ("memories", "id,data"), ("connections", "id,token_hash,data"),
        ("jobs", "path,root_id,force,attempts,available_at"),
        ("events", "id,created_at,kind,message,details"),
    ] {
        let kind: Option<String> = source.query_row("SELECT type FROM sqlite_schema WHERE name=?1", [table], |row| row.get(0)).optional()?;
        if kind.as_deref() != Some("table") { bail!("备份缺少知识库数据表：{table}，当前知识库未修改"); }
        source.prepare(&format!("SELECT {columns} FROM {table} LIMIT 0"))
            .with_context(|| format!("备份中的 {table} 结构不兼容，当前知识库未修改"))?;
    }
    validate_records::<Root>(source, "roots")?;
    validate_records::<Project>(source, "projects")?;
    validate_records::<Document>(source, "documents")?;
    validate_records::<Memory>(source, "memories")?;
    {
        let mut statement = source.prepare("SELECT location FROM chunks")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let _: Location = serde_json::from_str(&row.get::<_, String>(0)?)
                .context("备份中的资料位置格式无效，当前知识库未修改")?;
        }
        let mut statement = source.prepare("SELECT value FROM metadata")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? { let _: serde_json::Value = serde_json::from_str(&row.get::<_, String>(0)?)?; }
    }
    let raw: String = source.query_row("SELECT value FROM metadata WHERE key='settings'", [], |row| row.get(0))
        .context("备份缺少知识库设置，当前知识库未修改")?;
    let mut settings: Settings = serde_json::from_str(&raw).context("备份中的设置格式无效，当前知识库未修改")?;
    settings.paused = true;
    for provider in [&mut settings.chat, &mut settings.embedding, &mut settings.rerank] { provider.has_key = false; provider.credential_id = None; }
    let normalized = crate::store::Store::open(target_path)?;
    {
        let mut db = normalized.connection.lock().unwrap();
        db.execute("ATTACH DATABASE ?1 AS imported", [source_path.to_string_lossy().as_ref()])?;
        let transaction = db.transaction()?;
        transaction.execute_batch("INSERT OR REPLACE INTO metadata SELECT key,value FROM imported.metadata WHERE key NOT IN ('http_port','demo_created','credential_namespace');")?;
        for (table, columns) in [("roots","id,path,data"),("projects","id,path,data"),("memories","id,data")] {
            transaction.execute_batch(&format!("INSERT INTO {table}({columns}) SELECT {columns} FROM imported.{table};"))?;
        }
        // Pending deletions are not library content and their transient cleanup
        // queue must never be revived by a restored backup.
        transaction.execute_batch("INSERT INTO documents(id,path,root_id,project_id,version,status,data) SELECT id,path,root_id,project_id,version,status,data FROM imported.documents WHERE status!='removing';
            INSERT INTO chunks(rowid,id,document_id,ordinal,text,location,lexical) SELECT c.rowid,c.id,c.document_id,c.ordinal,c.text,c.location,c.lexical FROM imported.chunks c JOIN documents d ON d.id=c.document_id;
            INSERT INTO embeddings(chunk_id,model,dimensions,vector) SELECT e.chunk_id,e.model,e.dimensions,e.vector FROM imported.embeddings e JOIN chunks c ON c.id=e.chunk_id;")?;
        let orphan: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM documents d LEFT JOIN roots r ON r.id=d.root_id LEFT JOIN projects p ON p.id=d.project_id WHERE r.id IS NULL OR p.id IS NULL)", [], |row| row.get(0))?;
        if orphan { bail!("备份包含缺少目录或项目的文档，当前知识库未修改"); }
        let invalid_vector: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM embeddings WHERE dimensions<=0 OR length(vector)!=dimensions*4)", [], |row| row.get(0))?;
        if invalid_vector { bail!("备份中的向量格式无效，当前知识库未修改"); }
        transaction.commit()?;
        db.execute_batch("DETACH DATABASE imported;")?;
    }
    normalized.set_metadata("settings", &settings)?;
    normalized.set_metadata("credential_namespace", &namespace)?;
    for mut root in normalized.roots()? {
        crate::indexer::validate_exclusions(&root.exclusions)?;
        root.enabled = false;
        root.error = Some("备份已还原，请确认或重新关联原目录后开启同步。".into());
        normalized.save_root(&root)?;
    }
    let appearance = crate::appearance::load(&normalized)?;
    crate::appearance::update(&normalized, serde_json::to_value(appearance)?)?;
    crate::onboarding::load(&normalized)?;
    normalized.connection.lock().unwrap().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    Ok(())
}

pub async fn restore(state: &Arc<AppState>, bytes: Vec<u8>) -> Result<serde_json::Value> {
    let namespace = state.store.metadata::<String>("credential_namespace")?.unwrap_or_else(|| crate::storage::credential_namespace(&state.home));
    if bytes.len() > 512 * 1024 * 1024 { bail!("备份文件超过 512 MB，请使用存储迁移功能"); }
    let mut archive = ZipArchive::new(Cursor::new(bytes)).context("备份文件无法打开")?;
    let mut manifest = String::new();
    archive.by_name("manifest.json")?.take(65536).read_to_string(&mut manifest)?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest)?;
    if manifest["format"] != "personal-knowledge-backup" || manifest["schema"] != 1 { bail!("此备份格式暂不支持"); }
    let entry = archive.by_name("knowledge.sqlite")?;
    if entry.size() > crate::storage::GIB { bail!("备份数据库过大"); }
    let current_size = std::fs::metadata(state.home.join("knowledge.sqlite"))?.len();
    room_for_temporary(state, entry.size().saturating_mul(2).saturating_add(current_size.saturating_mul(2)).saturating_add(64*1024*1024))?;
    let temporary = state.home.join(format!("restore-{}.sqlite", identifier()));
    let normalized = state.home.join(format!("restore-normalized-{}.sqlite", identifier()));
    let _input_cleanup = Cleanup(temporary.clone());
    let _normalized_cleanup = Cleanup(normalized.clone());
    {
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
        let mut file = options.open(&temporary)?;
        let count = std::io::copy(&mut entry.take(crate::storage::GIB + 1), &mut file)?;
        if count > crate::storage::GIB { bail!("解压后的备份数据库超过 1 GB，当前知识库未修改"); }
    }
    let input = Sqlite::open_with_flags(&temporary, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let integrity: String = input.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    let version: i64 = input.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if integrity != "ok" || version != 1 { bail!("备份完整性或数据库版本检查失败，当前知识库未修改"); }
    normalize_restore(&input, &temporary, &normalized, &namespace)?;
    drop(input);
    let source = Sqlite::open_with_flags(&normalized, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let _maintenance = state.maintenance.write().await;
    let recovery = export_locked(state)?;
    let recovery_path = state.home.join("backups").join(format!("before-restore-{}.pkb", identifier()));
    private_write(&recovery_path, &recovery)?;
    {
        let mut target = state.store.connection.lock().unwrap();
        let backup = Backup::new(&source, &mut target)?;
        backup.run_to_completion(100, Duration::from_millis(10), None)?;
    }
    state.store.reload_clients()?;
    // All normalization was completed before the atomic copy, so a subsequent
    // log failure must not turn a completed restore into a false failure.
    if let Err(error) = state.store.event("restore", "备份已还原，等待重新关联目录", json!({"recovery_path":recovery_path})) {
        tracing::warn!(%error, "restore committed but activity could not be written");
    }
    state.wake.notify_one();
    Ok(json!({"ok":true,"recovery_path":recovery_path,"message":"备份已还原。请重新关联资料目录、填写模型凭据并创建 Agent 连接。"}))
}

#[cfg(test)]
mod review_tests {
    use super::*;

    fn archive_database(path: &std::path::Path) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(br#"{"format":"personal-knowledge-backup","schema":1}"#).unwrap();
        zip.start_file("knowledge.sqlite", options).unwrap();
        zip.write_all(&std::fs::read(path).unwrap()).unwrap();
        zip.finish().unwrap().into_inner()
    }

    #[tokio::test]
    async fn a_valid_sqlite_file_without_the_app_schema_never_replaces_the_library() {
        let directory = tempfile::tempdir().unwrap();
        let state = AppState::new(directory.path().join("current"), RuntimeConfig::default()).unwrap();
        let original = state.store.ensure_project("/original", "original-project").unwrap();
        let input = directory.path().join("unrelated.sqlite");
        {
            let db = Sqlite::open(&input).unwrap();
            db.execute_batch("PRAGMA user_version=1; CREATE TABLE unrelated(value TEXT);").unwrap();
        }
        assert!(restore(&state, archive_database(&input)).await.is_err());
        assert_eq!(state.store.projects().unwrap()[0].id, original.id);
        assert!(!state.store.settings().unwrap().paused);
        assert_eq!(state.store.stats().unwrap()["projects"], 1);
    }

    #[tokio::test]
    async fn malformed_backup_records_are_rejected_before_any_live_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let state = AppState::new(directory.path().join("current"), RuntimeConfig::default()).unwrap();
        let original = state.store.ensure_project("/original", "original-project").unwrap();
        let input = directory.path().join("invalid-record.sqlite");
        {
            let store = crate::store::Store::open(&input).unwrap();
            store.set_metadata("settings", &Settings::default()).unwrap();
            store.connection.lock().unwrap().execute("INSERT INTO projects(id,path,data) VALUES('bad','/bad','not-json')", []).unwrap();
        }
        assert!(restore(&state, archive_database(&input)).await.is_err());
        assert_eq!(state.store.projects().unwrap()[0].id, original.id);
    }

    #[tokio::test]
    async fn restored_schema_is_canonical_and_input_triggers_and_credentials_are_not_installed() {
        let directory = tempfile::tempdir().unwrap();
        let state = AppState::new(directory.path().join("current"), RuntimeConfig::default()).unwrap();
        let input = directory.path().join("valid-data.sqlite");
        let project_id;
        {
            let store = crate::store::Store::open(&input).unwrap();
            let mut settings = Settings::default();
            settings.chat.has_key = true;
            settings.chat.credential_id = Some("not-a-real-credential".into());
            store.set_metadata("settings", &settings).unwrap();
            project_id = store.ensure_project("/restored", "restored-project").unwrap().id;
            store.save_root(&Root {id:"root".into(),path:"/restored".into(),name:"restored".into(),policy:"agents".into(),enabled:true,exclusions:vec![],created_at:timestamp(),last_scan:None,error:None}).unwrap();
            store.connection.lock().unwrap().execute_batch("CREATE TRIGGER unexpected_root_update AFTER UPDATE ON roots BEGIN DELETE FROM projects; END;
                INSERT INTO connections(id,token_hash,data) VALUES('untrusted','untrusted','not-json');").unwrap();
        }
        restore(&state, archive_database(&input)).await.unwrap();
        assert_eq!(state.store.projects().unwrap()[0].id, project_id);
        assert!(state.store.roots().unwrap().iter().all(|root| !root.enabled));
        assert!(state.store.connections().unwrap().is_empty());
        assert!(!state.store.settings().unwrap().chat.has_key);
        let db = state.store.connection.lock().unwrap();
        let count: u64 = db.query_row("SELECT COUNT(*) FROM sqlite_schema WHERE name='unexpected_root_update'", [], |row|row.get(0)).unwrap();
        assert_eq!(count, 0);
    }
}

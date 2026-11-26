use crate::{connections::private_write, state::AppState, types::*};
use anyhow::{bail, Context, Result};
use rusqlite::{backup::Backup, Connection as Sqlite};
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

pub async fn restore(state: &Arc<AppState>, bytes: Vec<u8>) -> Result<serde_json::Value> {
    let namespace=state.store.metadata::<String>("credential_namespace")?.unwrap_or_else(||crate::storage::credential_namespace(&state.home));
    if bytes.len() > 512 * 1024 * 1024 {
        bail!("备份文件超过 512 MB，请使用分卷或联系维护人员");
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes)).context("备份文件无法打开")?;
    let mut manifest = String::new();
    archive
        .by_name("manifest.json")?
        .take(65536)
        .read_to_string(&mut manifest)?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest)?;
    if manifest["format"] != "personal-knowledge-backup" || manifest["schema"] != 1 {
        bail!("此备份格式暂不支持");
    }
    let mut database = vec![];
    let entry = archive.by_name("knowledge.sqlite")?;
    if entry.size() > 1024 * 1024 * 1024 {
        bail!("备份数据库过大");
    }
    entry
        .take(1024 * 1024 * 1024 + 1)
        .read_to_end(&mut database)?;
    room_for_temporary(state, (database.len() as u64).saturating_mul(3))?;
    let temporary = state.home.join(format!("restore-{}.sqlite", identifier()));
    let _cleanup = Cleanup(temporary.clone());
    private_write(&temporary, &database)?;
    let source = Sqlite::open_with_flags(&temporary, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let integrity: String = source.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    let version: i64 = source.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if integrity != "ok" || version != 1 {
        let _ = std::fs::remove_file(&temporary);
        bail!("备份完整性或数据库版本检查失败");
    }
    let _maintenance = state.maintenance.write().await;
    let recovery = export_locked(state)?;
    let recovery_path = state.home.join("backups").join(format!(
        "before-restore-{}.pkb",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    ));
    private_write(&recovery_path, &recovery)?;
    {
        let mut target = state.store.connection.lock().unwrap();
        let backup = Backup::new(&source, &mut target)?;
        backup.run_to_completion(100, Duration::from_millis(10), None)?;
    }
    state.store.reload_clients()?;
    state.store.set_metadata("credential_namespace",&namespace)?;
    state.store.ensure_fts_triggers()?;
    {
        let db = state.store.connection.lock().unwrap();
        crate::counters::ensure(&db, false)?;
    }
    crate::counters::backfill(&state.store)?;
    for mut root in state.store.roots()? {
        root.enabled = false;
        root.error = Some("备份已还原，请确认或重新关联原目录后开启同步。".into());
        state.store.save_root(&root)?;
    }
    let mut settings = state.store.settings()?;
    settings.paused = true;
    settings.chat.has_key = false;
    settings.embedding.has_key = false;
    settings.rerank.has_key = false;
    settings.chat.credential_id = None;
    settings.embedding.credential_id = None;
    settings.rerank.credential_id = None;
    state.store.set_metadata("settings", &settings)?;
    state.store.event(
        "restore",
        "备份已还原，等待重新关联目录",
        json!({"recovery_path":recovery_path}),
    )?;
    drop(source);
    let _ = std::fs::remove_file(&temporary);
    state.wake.notify_one();
    Ok(
        json!({"ok":true,"recovery_path":recovery_path,"message":"备份已还原。请重新关联资料目录、填写模型凭据并创建 Agent 连接。"}),
    )
}

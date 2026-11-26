use crate::{connections::private_write, types::*};
use anyhow::{bail, Context, Result};
use fs2::FileExt;
use rusqlite::{backup::Backup, Connection as Sqlite, OpenFlags};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub const GIB: u64 = 1024 * 1024 * 1024;
pub const LOCATION_FILE: &str = "storage-location.json";

pub fn error_message(error: &anyhow::Error) -> String {
    if error.chain().any(|cause| {
        cause
            .to_string()
            .to_ascii_lowercase()
            .contains("database or disk is full")
    }) {
        "知识库存储盘已满，索引需要暂停。请打开设置 → 存储位置与容量，迁移到其他磁盘或释放空间；不要直接删除数据库日志文件。".into()
    } else {
        error.to_string()
    }
}
const FILES: &[&str] = &[
    "knowledge.sqlite",
    "knowledge.sqlite-wal",
    "knowledge.sqlite-shm",
    "agent-tokens.json",
    "runtime.json",
    "service.log",
    "service.log.1",
];
const DIRECTORIES: &[&str] = &["backups", "config-backups"];
const CACHE_DIRECTORIES: &[&str] = &[
    "browser",
    "tmp",
    "logs",
    "crashes",
    "Cache",
    "Code Cache",
    "GPUCache",
    "DawnGraphiteCache",
    "DawnWebGPUCache",
    "blob_storage",
    "Session Storage",
    "Local Storage",
    "Network",
    "Crashpad",
];

#[derive(Serialize, Debug)]
pub struct StorageStatus {
    pub available_bytes: u64,
    pub index_bytes: u64,
    pub wal_bytes: u64,
    pub reserve_bytes: u64,
    pub limit_bytes: u64,
    pub blocked: bool,
    pub reason: Option<String>,
}

pub fn policy(
    available: u64,
    index: u64,
    wal: u64,
    settings: &Settings,
    growth: u64,
) -> StorageStatus {
    let reserve = settings.min_free_disk_gb.clamp(2, 100) * GIB;
    let limit = settings.max_index_gb.clamp(1, 1024) * GIB;
    let reason = if available < reserve.saturating_add(growth) {
        Some(format!("存储盘空间不足，索引已自动暂停。需保留至少 {} GB 可用空间，请迁移数据或释放空间后继续。", settings.min_free_disk_gb.clamp(2, 100)))
    } else if index.saturating_add(growth) >= limit {
        Some(format!(
            "索引已达到 {} GB 容量上限，已自动暂停。请管理资料范围或调整容量上限。",
            settings.max_index_gb.clamp(1, 1024)
        ))
    } else {
        None
    };
    StorageStatus {
        available_bytes: available,
        index_bytes: index,
        wal_bytes: wal,
        reserve_bytes: reserve,
        limit_bytes: limit,
        blocked: reason.is_some(),
        reason,
    }
}

fn size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

pub fn status(home: &Path, settings: &Settings, growth: u64) -> Result<StorageStatus> {
    let wal = size(&home.join("knowledge.sqlite-wal"));
    let index = size(&home.join("knowledge.sqlite"))
        .saturating_add(wal)
        .saturating_add(size(&home.join("knowledge.sqlite-shm")));
    Ok(policy(
        fs2::available_space(home).context("无法读取存储盘剩余空间，索引已暂停")?,
        index,
        wal,
        settings,
        growth,
    ))
}

pub fn require_capacity(home: &Path, settings: &Settings, growth: u64) -> Result<()> {
    if let Some(reason) = status(home, settings, growth)?.reason {
        bail!("{reason}");
    }
    Ok(())
}

pub fn credential_namespace(home: &Path) -> String {
    let fingerprint = blake3::hash(home.to_string_lossy().as_bytes())
        .to_hex()
        .to_string();
    format!("personal-knowledge-{}", &fingerprint[..16])
}

// Old MCP configurations keep working after a move. A missing drive must never
// silently create an empty knowledge base in the original directory.
pub fn resolve_home(home: &Path) -> Result<PathBuf> {
    let mut current = home.to_path_buf();
    let mut visited = std::collections::HashSet::new();
    for _ in 0..8 {
        if !visited.insert(current.clone()) {
            bail!("存储目录指向形成循环，请联系维护者");
        }
        let location = current.join(LOCATION_FILE);
        if !location.exists() {
            return Ok(current);
        }
        let value: Value = serde_json::from_slice(&std::fs::read(location)?)?;
        let next = PathBuf::from(
            value["data_directory"]
                .as_str()
                .context("存储位置配置损坏")?,
        );
        if !next.is_absolute() || !next.is_dir() {
            bail!(
                "知识库存储目录不可用，请连接原数据盘后重新启动：{}",
                next.display()
            );
        }
        if next == current {
            return Ok(current);
        }
        current = next;
    }
    bail!("存储目录转移次数过多，请重新配置 Agent 连接")
}

fn copy_directory(source: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            bail!("数据目录含符号链接，已停止迁移以保留原数据");
        }
        if kind.is_dir() {
            copy_directory(&entry.path(), &target.join(entry.file_name()))?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), target.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn managed_bytes(home: &Path) -> Result<u64> {
    let mut total = FILES.iter().map(|name| size(&home.join(name))).sum::<u64>();
    for name in DIRECTORIES {
        for entry in walkdir::WalkDir::new(home.join(name)).follow_links(false) {
            let Ok(entry) = entry else { continue };
            if entry.file_type().is_symlink() {
                bail!("数据目录含符号链接，请先处理后再迁移");
            }
            if entry.file_type().is_file() {
                total = total.saturating_add(entry.metadata()?.len());
            }
        }
    }
    Ok(total)
}

pub fn validate_destination(source: &Path, destination: &Path) -> Result<PathBuf> {
    if !destination.is_absolute() {
        bail!("请选择完整的数据目录路径");
    }
    let parent = destination
        .parent()
        .context("请选择磁盘内的文件夹，而非整块磁盘")?
        .canonicalize()
        .context("目标位置无法访问")?;
    let destination = parent.join(destination.file_name().context("目标目录无效")?);
    let source = source.canonicalize()?;
    if destination.starts_with(&source) || source.starts_with(&destination) {
        bail!("新旧数据目录不能相同或互相包含");
    }
    if destination.exists()
        && (!destination.is_dir() || std::fs::read_dir(&destination)?.next().is_some())
    {
        bail!("目标目录已有内容，请选择一个新的空目录；不会覆盖或合并现有资料");
    }
    if source.join("knowledge.sqlite").is_file() {
        let db = Sqlite::open_with_flags(
            source.join("knowledge.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let mut statement = db.prepare("SELECT path FROM roots")?;
        for path in statement.query_map([], |row| row.get::<_, String>(0))? {
            let path = PathBuf::from(path?);
            let path = path.canonicalize().unwrap_or(path);
            let path = path.canonicalize().unwrap_or(path);
            if destination.starts_with(&path) || path.starts_with(&destination) {
                bail!("数据目录不能位于已索引的资料范围中，请选择资料目录之外的位置");
            }
        }
    }
    let needed = managed_bytes(&source)?.saturating_add(5 * GIB);
    if fs2::available_space(&parent)? < needed {
        bail!("目标磁盘空间不足：需要容纳现有数据并额外保留 5 GB 空间");
    }
    Ok(destination)
}

// Requires an offline service lock. SQLite's backup API includes committed WAL
// contents without requiring a second full database copy on a full source disk.
pub fn migrate(source: &Path, destination: &Path) -> Result<Value> {
    let source = source.canonicalize()?;
    let destination = validate_destination(&source, destination)?;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(source.join("service.lock"))?;
    lock.try_lock_exclusive()
        .context("知识服务尚未退出，请关闭 Agent 的知识库连接后重试")?;
    if fs2::available_space(destination.parent().unwrap())?
        < managed_bytes(&source)?.saturating_add(5 * GIB)
    {
        bail!("目标盘空间不足，需容纳现有知识库并额外保留至少 5 GB");
    }
    let staging = destination.with_file_name(format!(".knowledge-migrating-{}", identifier()));
    std::fs::create_dir(&staging)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o700))?;
    }
    let result = (|| -> Result<()> {
        if source.join("knowledge.sqlite").is_file() {
            let original = Sqlite::open_with_flags(
                source.join("knowledge.sqlite"),
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            let mut target = Sqlite::open(staging.join("knowledge.sqlite"))?;
            {
                let backup = Backup::new(&original, &mut target)?;
                backup.run_to_completion(256, Duration::from_millis(5), None)?;
            }
            let integrity: String =
                target.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
            if integrity != "ok" {
                bail!("迁移副本完整性检查失败，原始数据未删除");
            }
            let namespace = serde_json::to_string(&credential_namespace(&source))?;
            target.execute(
                "INSERT OR IGNORE INTO metadata(key,value) VALUES('credential_namespace',?1)",
                [namespace],
            )?;
            let settings: String = target.query_row(
                "SELECT value FROM metadata WHERE key='settings'",
                [],
                |row| row.get(0),
            )?;
            let mut settings: Settings = serde_json::from_str(&settings)?;
            settings.paused = true;
            target.execute(
                "UPDATE metadata SET value=?1 WHERE key='settings'",
                [serde_json::to_string(&settings)?],
            )?;
            // Source documents live outside the managed directory and remain in place.
            // This validation prevents a data-folder move from ingesting its own index.
            target.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
            drop(target);
        }
        for name in FILES
            .iter()
            .filter(|name| !name.starts_with("knowledge.sqlite"))
        {
            let path = source.join(name);
            if path.is_file() {
                std::fs::copy(path, staging.join(name))?;
            }
        }
        for name in DIRECTORIES {
            if source.join(name).is_dir() {
                copy_directory(&source.join(name), &staging.join(name))?;
            }
        }
        private_write(
            &staging.join("migration-source.json"),
            &serde_json::to_vec(&json!({"source":source}))?,
        )?;
        if destination.exists() {
            std::fs::remove_dir(&destination).context("目标目录已出现新内容，迁移已停止")?;
        }
        std::fs::rename(&staging, &destination)?;
        private_write(
            &source.join(LOCATION_FILE),
            &serde_json::to_vec(&json!({"version":1,"data_directory":destination}))?,
        )?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result?;
    Ok(json!({"data_directory":destination,"source":source,"paused":true}))
}

// Called only after the relocated service has successfully started. Delete only
// known application-owned files; keep the redirect and any unrecognized files.
pub fn finish_migration(home: &Path) -> Result<()> {
    let pending = home.join("migration-source.json");
    if !pending.is_file() {
        return Ok(());
    }
    let value: Value = serde_json::from_slice(&std::fs::read(&pending)?)?;
    let source =
        PathBuf::from(value["source"].as_str().context("迁移来源记录损坏")?).canonicalize()?;
    if resolve_home(&source)?.canonicalize()? != home.canonicalize()? {
        bail!("迁移指向不一致，未清理旧数据");
    }
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(source.join("service.lock"))?;
    lock.try_lock_exclusive()
        .context("旧数据目录仍被占用，暂未清理")?;
    for name in FILES {
        let path = source.join(name);
        if path.is_file() {
            std::fs::remove_file(path)?;
        }
    }
    for name in DIRECTORIES.iter().chain(CACHE_DIRECTORIES) {
        let path = source.join(name);
        if path.is_dir() {
            std::fs::remove_dir_all(path)?;
        }
    }
    for entry in std::fs::read_dir(&source)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if ((name.starts_with("backup-") || name.starts_with("restore-"))
            && name.ends_with(".sqlite"))
            || name == "service.json"
        {
            if entry.file_type()?.is_file() {
                std::fs::remove_file(entry.path())?;
            }
        }
    }
    std::fs::remove_file(pending)?;
    Ok(())
}

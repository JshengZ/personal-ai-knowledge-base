use crate::{connections::private_write, types::*};
use anyhow::{bail, Context, Result};
use fs2::FileExt;
use rusqlite::{backup::Backup, Connection as Sqlite, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

pub const GIB: u64 = 1024 * 1024 * 1024;
pub const LOCATION_FILE: &str = "storage-location.json";
const MIGRATION_JOB: &str = "migration-job.json";
const MIGRATION_OWNER: &str = ".migration-owner.json";
const MIGRATION_APP: &str = "personal-knowledge";
const TRACKED_TABLES: &[&str] = &["chunks", "embeddings"];

pub fn migration_pending(home: &Path) -> bool {
    home.join(MIGRATION_JOB).exists()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct FileProof {
    path: PathBuf,
    bytes: u64,
    digest: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct MigrationSnapshot {
    database: Option<String>,
    files: Vec<FileProof>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MigrationJob {
    version: u32,
    app: String,
    job_id: String,
    source: PathBuf,
    destination: PathBuf,
    phase: String,
    owner_pid: u32,
    created_at: String,
    snapshot: MigrationSnapshot,
}

impl MigrationJob {
    fn stage(&self) -> PathBuf {
        self.destination
            .with_file_name(format!(".knowledge-migrating-{}", self.job_id))
    }

    fn matches(&self, other: &Self) -> bool {
        self.version == other.version
            && self.app == other.app
            && self.job_id == other.job_id
            && self.source == other.source
            && self.destination == other.destination
            && self.snapshot == other.snapshot
    }
}

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
    /// Committed live pages plus the WAL. Free database pages can be reused.
    pub quota_bytes: u64,
    pub reusable_bytes: u64,
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
        quota_bytes: index,
        reusable_bytes: 0,
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
    let (live, reusable) = if home.join("knowledge.sqlite").is_file() {
        let db = Sqlite::open_with_flags(
            home.join("knowledge.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        db.busy_timeout(Duration::from_millis(100))?;
        // Read one consistent WAL snapshot; these pragmas inspect page metadata,
        // not every chunk. A DELETE does not shrink an SQLite database file.
        db.execute_batch("BEGIN")?;
        let page_size: u64 = db.query_row("PRAGMA page_size", [], |row| row.get(0))?;
        let pages: u64 = db.query_row("PRAGMA page_count", [], |row| row.get(0))?;
        let free: u64 = db.query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
        (
            pages.saturating_sub(free).saturating_mul(page_size),
            free.saturating_mul(page_size),
        )
    } else {
        (0, 0)
    };
    let mut result = policy(
        fs2::available_space(home).context("无法读取存储盘剩余空间，索引已暂停")?,
        live.saturating_add(wal),
        wal,
        settings,
        growth,
    );
    result.index_bytes = index;
    result.reusable_bytes = reusable;
    Ok(result)
}

/// Cleanup also writes a rollback-safe WAL. Allow bounded recovery below the
/// normal indexing reserve, but never let a large purge consume the last bytes.
pub fn require_cleanup_capacity(home: &Path, write_budget: u64) -> Result<()> {
    if size(&home.join("knowledge.sqlite-wal")) > 32 * 1024 * 1024 {
        bail!("索引清理正在等待其他读取释放数据库日志，请稍后重试；原始资料未删除");
    }
    if fs2::available_space(home)? < write_budget.saturating_add(32 * 1024 * 1024) {
        bail!("剩余空间不足以安全清理索引，请先释放至少 64 MB 空间或迁移数据；原始资料未删除");
    }
    Ok(())
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
            copy_file(&entry.path(), &target.join(entry.file_name()))?;
        }
    }
    #[cfg(unix)]
    std::fs::File::open(target)?.sync_all()?;
    Ok(())
}

fn copy_file(source: &Path, target: &Path) -> Result<()> {
    let mut reader = std::fs::File::open(source)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut writer = options.open(target)?;
    std::io::copy(&mut reader, &mut writer)?;
    // The durable ready marker must not get ahead of the actual copied bytes.
    writer.sync_all()?;
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

fn write_job(path: &Path, job: &MigrationJob) -> Result<()> {
    private_write(path, &serde_json::to_vec(job)?)
        .context("无法持久化迁移作业记录，请先释放源盘空间后重试；不会删除任何知识库副本")?;
    #[cfg(unix)]
    std::fs::File::open(path.parent().context("迁移作业路径无效")?)?.sync_all()?;
    Ok(())
}

fn read_job(path: &Path) -> Result<Option<MigrationJob>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let job: MigrationJob = serde_json::from_slice(&bytes)
        .context("迁移记录损坏，已保留所有副本；请勿删除记录或迁移目录，请联系维护者恢复")?;
    if job.version != 1
        || job.app != MIGRATION_APP
        || uuid::Uuid::parse_str(&job.job_id).is_err()
        || !job.source.is_absolute()
        || !job.destination.is_absolute()
        || !["copying", "ready", "published"].contains(&job.phase.as_str())
    {
        bail!("无法识别的迁移记录，已保留所有副本；请联系维护者恢复");
    }
    Ok(Some(job))
}

fn owned_copy(path: &Path, job: &MigrationJob) -> Result<Option<MigrationJob>> {
    if !path.exists() {
        return Ok(None);
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        bail!("迁移位置含未知文件或链接，已保留原状：{}", path.display());
    }
    let owner = read_job(&path.join(MIGRATION_OWNER))?
        .context("迁移位置已有未能确认归属的内容；已保留原状，请选择新的空目录或联系维护者")?;
    if !job.matches(&owner) {
        bail!("迁移副本的作业记录不匹配，已保留所有文件；请选择新的空目录或联系维护者");
    }
    Ok(Some(owner))
}

fn hash_file(path: &Path) -> Result<(u64, String)> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!(
            "迁移受管文件含链接或未知类型，已停止并保留原数据：{}",
            path.display()
        );
    }
    let mut file = std::fs::File::open(path)?;
    let mut digest = blake3::Hasher::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        bytes += count as u64;
    }
    Ok((bytes, digest.finalize().to_hex().to_string()))
}

// Hash credentials and saved backups, but not ephemeral logs/runtime paths.
// The SQLite proof tracks chunk/vector mutations without hashing every float.
fn file_proofs(home: &Path) -> Result<Vec<FileProof>> {
    let mut paths = Vec::new();
    if home.join("agent-tokens.json").exists() {
        paths.push(PathBuf::from("agent-tokens.json"));
    }
    for name in DIRECTORIES {
        let directory = home.join(name);
        if !directory.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&directory).follow_links(false) {
            let entry = entry?;
            if entry.file_type().is_symlink() {
                bail!("迁移受管目录含链接，已保留所有副本");
            }
            if entry.file_type().is_file() {
                paths.push(entry.path().strip_prefix(home)?.to_path_buf());
            }
        }
    }
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let (bytes, digest) = hash_file(&home.join(&path))?;
            Ok(FileProof {
                path,
                bytes,
                digest,
            })
        })
        .collect()
}

fn database_proof(home: &Path, check_integrity: bool) -> Result<Option<String>> {
    let path = home.join("knowledge.sqlite");
    if !path.exists() {
        return Ok(None);
    }
    if std::fs::symlink_metadata(&path)?.file_type().is_symlink() {
        bail!("数据库文件是链接，已停止迁移并保留所有副本");
    }
    let db = Sqlite::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    db.busy_timeout(Duration::from_millis(100))?;
    db.execute_batch("BEGIN")?;
    if check_integrity {
        let integrity: String = db.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            bail!("源数据库未通过完整性检查，所有副本已保留，请联系维护者恢复");
        }
    }
    let mut digest = blake3::Hasher::new();
    for (table, query) in [
        ("roots", "SELECT id,path,data FROM roots ORDER BY id"),
        ("projects", "SELECT id,path,data FROM projects ORDER BY id"),
        (
            "documents",
            "SELECT id,path,root_id,project_id,version,status,data FROM documents ORDER BY id",
        ),
        ("memories", "SELECT id,data FROM memories ORDER BY id"),
        (
            "connections",
            "SELECT id,token_hash,data FROM connections ORDER BY id",
        ),
        (
            "jobs",
            "SELECT path,root_id,force,attempts,available_at FROM jobs ORDER BY path",
        ),
    ] {
        digest.update(table.as_bytes());
        let mut statement = db.prepare(query)?;
        let columns = statement.column_count();
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            for column in 0..columns {
                let value = row.get_ref(column)?;
                let bytes = match value {
                    rusqlite::types::ValueRef::Text(bytes)
                    | rusqlite::types::ValueRef::Blob(bytes) => bytes.to_vec(),
                    rusqlite::types::ValueRef::Integer(value) => value.to_le_bytes().to_vec(),
                    rusqlite::types::ValueRef::Real(value) => value.to_le_bytes().to_vec(),
                    rusqlite::types::ValueRef::Null => vec![],
                };
                digest.update(&(bytes.len() as u64).to_le_bytes());
                digest.update(&bytes);
            }
        }
    }
    let mut statement = db.prepare("SELECT key,value FROM metadata ORDER BY key")?;
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })? {
        let (key, value) = row?;
        if key == "http_port" || key == "last_scan" || key.starts_with("scan_") {
            continue;
        }
        let mut value: Value =
            serde_json::from_str(&value).context("源数据库设置损坏，已保留所有副本")?;
        if key == "settings" {
            let mut settings: Settings = serde_json::from_value(value)?;
            settings.paused = true;
            value = serde_json::to_value(settings)?;
        }
        digest.update(&(key.len() as u64).to_le_bytes());
        digest.update(key.as_bytes());
        let value = serde_json::to_vec(&value)?;
        digest.update(&(value.len() as u64).to_le_bytes());
        digest.update(&value);
    }
    // The same job id/revision must survive a service restart or backup copy.
    // Restore into a new canonical database loses these sentinels and is refused.
    let sentinels: i64 = db.query_row(
        "SELECT COUNT(*) FROM metadata WHERE key IN ('migration_job_id','migration_revision')",
        [],
        |row| row.get(0),
    )?;
    if sentinels != 2 {
        bail!("迁移所需的源数据库版本记录缺失，请选择新空目录重新迁移；原副本已保留");
    }
    let cleanup: i64 = db.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cleanup_jobs'",
        [],
        |row| row.get(0),
    )?;
    if cleanup != 0 {
        let mut statement = db
            .prepare("SELECT document_id,version,root_id FROM cleanup_jobs ORDER BY document_id")?;
        for row in statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })? {
            let bytes = serde_json::to_vec(&row?)?;
            digest.update(&(bytes.len() as u64).to_le_bytes());
            digest.update(&bytes);
        }
    }
    Ok(Some(digest.finalize().to_hex().to_string()))
}

fn snapshot(home: &Path, check_integrity: bool) -> Result<MigrationSnapshot> {
    Ok(MigrationSnapshot {
        database: database_proof(home, check_integrity)?,
        files: file_proofs(home)?,
    })
}

fn install_tracking(home: &Path, job_id: &str) -> Result<()> {
    if !home.join("knowledge.sqlite").exists() {
        return Ok(());
    }
    let mut db = Sqlite::open_with_flags(
        home.join("knowledge.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?;
    let tx = db.transaction()?;
    tx.execute(
        "INSERT OR IGNORE INTO metadata(key,value) VALUES('credential_namespace',?1)",
        [serde_json::to_string(&credential_namespace(home))?],
    )?;
    // Keep an existing tracking epoch/revision when an explicit new target
    // archives an older job. A failure to save the new journal must not make
    // an otherwise unchanged older complete copy impossible to recover.
    tx.execute(
        "INSERT OR IGNORE INTO metadata(key,value) VALUES('migration_job_id',?1)",
        [serde_json::to_string(job_id)?],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO metadata(key,value) VALUES('migration_revision','0')",
        [],
    )?;
    for table in TRACKED_TABLES {
        for action in ["INSERT", "UPDATE", "DELETE"] {
            tx.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS migration_track_{table}_{action} AFTER {action} ON {table} BEGIN UPDATE metadata SET value=CAST(value AS INTEGER)+1 WHERE key='migration_revision'; END;"))?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn remove_tracking(home: &Path) -> Result<()> {
    if !home.join("knowledge.sqlite").is_file() {
        return Ok(());
    }
    let mut db = Sqlite::open_with_flags(
        home.join("knowledge.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?;
    let tx = db.transaction()?;
    for table in TRACKED_TABLES {
        for action in ["INSERT", "UPDATE", "DELETE"] {
            tx.execute_batch(&format!(
                "DROP TRIGGER IF EXISTS migration_track_{table}_{action}"
            ))?;
        }
    }
    tx.execute(
        "DELETE FROM metadata WHERE key IN ('migration_job_id','migration_revision')",
        [],
    )?;
    tx.commit()?;
    Ok(())
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
    if std::fs::symlink_metadata(&destination)
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        bail!("目标位置是链接，请选择新的真实空目录");
    }
    let destination = if destination.exists() {
        destination.canonicalize()?
    } else {
        destination
    };
    let source = source.canonicalize()?;
    if destination.starts_with(&source) || source.starts_with(&destination) {
        bail!("新旧数据目录不能相同或互相包含");
    }
    let pending = read_job(&source.join(MIGRATION_JOB))?;
    if pending.as_ref().is_some_and(|job| job.source != source) {
        bail!("迁移记录与当前数据位置不一致，已保留所有副本，请联系维护者");
    }
    let same_job = pending
        .as_ref()
        .filter(|job| job.destination == destination);
    let mut reclaimable = 0;
    let resumable = if let Some(job) = same_job {
        if destination.exists() && std::fs::read_dir(&destination)?.next().is_some() {
            let owner = owned_copy(&destination, job)?.context("迁移副本缺失")?;
            if owner.phase == "copying" {
                bail!("目标位置不是已验证的完整副本，所有数据已保留，请联系维护者");
            }
            true
        } else {
            let stage = owned_copy(&job.stage(), job)?;
            if stage.as_ref().is_some_and(|owner| owner.phase == "copying") {
                // Validation cannot remove a stage while the source service is
                // running. Account for space the locked retry may safely free.
                reclaimable = managed_bytes(&job.stage())?;
            }
            stage.is_some_and(|owner| owner.phase != "copying")
        }
    } else {
        false
    };
    if !resumable
        && destination.exists()
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
            if destination.starts_with(&path) || path.starts_with(&destination) {
                bail!("数据目录不能位于已索引的资料范围中，请选择资料目录之外的位置");
            }
        }
    }
    // A complete owned copy only needs its redirect committed. Requiring a
    // second full copy here would strand large migrations after interruption.
    let needed = if resumable {
        16 * 1024 * 1024
    } else {
        managed_bytes(&source)?.saturating_add(5 * GIB)
    };
    if fs2::available_space(&parent)?.saturating_add(reclaimable) < needed {
        bail!("目标磁盘空间不足：新迁移需容纳现有数据并额外保留 5 GB 空间");
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
    // Recheck all paths after obtaining the source lock. Nothing below touches
    // an old stage until ownership and the still-complete source are proven.
    let destination = validate_destination(&source, &destination)?;
    if source.join("migration-source.json").exists() {
        bail!("上次迁移的旧位置尚未清理，请重新启动应用完成清理后再迁移；所有副本已保留");
    }
    let previous = read_job(&source.join(MIGRATION_JOB))?;
    let mut job = if let Some(previous) = previous.filter(|job| job.destination == destination) {
        let current = snapshot(&source, true)
            .context("无法确认源知识库仍完整，所有副本已保留；请联系维护者恢复")?;
        if current != previous.snapshot {
            bail!("源知识库在上次迁移后已改变，不能激活旧副本。所有副本均已保留；请选择另一个新的空目录重新迁移，或联系维护者使用迁移作业记录恢复");
        }
        previous
    } else {
        let previous = read_job(&source.join(MIGRATION_JOB))?;
        if let Some(previous) = &previous {
            // A new destination is an explicit fresh migration. Keep the old
            // job/copies recoverable instead of deleting or merging them.
            write_job(
                &source.join(format!("migration-history-{}.json", previous.job_id)),
                &previous,
            )?;
        }
        let job_id = identifier();
        install_tracking(&source, &job_id)
            .context("源盘无法建立迁移版本记录，请先释放空间后重试；尚未复制数据")?;
        let snapshot = snapshot(&source, true)?;
        let job = MigrationJob {
            version: 1,
            app: MIGRATION_APP.into(),
            job_id,
            source: source.clone(),
            destination: destination.clone(),
            phase: "copying".into(),
            owner_pid: std::process::id(),
            created_at: timestamp(),
            snapshot,
        };
        if let Err(error) = write_job(&source.join(MIGRATION_JOB), &job) {
            if previous.is_none() {
                let _ = remove_tracking(&source);
            }
            return Err(error.context("源盘无法保存迁移记录，尚未复制数据；请先释放空间"));
        }
        job
    };
    let staging = job.stage();
    if destination.exists() && std::fs::read_dir(&destination)?.next().is_some() {
        let owner = owned_copy(&destination, &job)?.context("迁移副本缺失")?;
        if owner.phase == "copying" {
            bail!("迁移目标不是已验证的完整副本，已保留所有文件，请联系维护者");
        }
        verify_copy(&destination, &job)?;
        return publish_migration(&source, &destination, &mut job);
    }
    if let Some(owner) = owned_copy(&staging, &job)? {
        if owner.phase != "copying" {
            verify_copy(&staging, &job)?;
            return rename_and_publish(&source, &destination, &staging, &mut job);
        }
        // The durable source snapshot is unchanged and readable, and the stage
        // is this exact job's incomplete copy. No possible sole copy is removed.
        std::fs::remove_dir_all(&staging).context("旧迁移暂存仍被占用，已保留数据，请稍后重试")?;
    }
    if fs2::available_space(destination.parent().unwrap())?
        < managed_bytes(&source)?.saturating_add(5 * GIB)
    {
        bail!("目标盘空间不足，需容纳现有知识库并额外保留至少 5 GB；迁移记录和原数据已保留");
    }
    job.phase = "copying".into();
    write_job(&source.join(MIGRATION_JOB), &job)?;
    std::fs::create_dir(&staging)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o700))?;
    }
    // Write ownership before any large file. An unowned stage is never removed
    // automatically, even if its directory name looks like ours.
    write_job(&staging.join(MIGRATION_OWNER), &job)?;
    let result = (|| -> Result<Value> {
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
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(staging.join("knowledge.sqlite"))?
                .sync_all()?;
        }
        for name in FILES
            .iter()
            .filter(|name| !name.starts_with("knowledge.sqlite"))
        {
            let path = source.join(name);
            if path.is_file() {
                copy_file(&path, &staging.join(name))?;
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
        verify_copy(&staging, &job)?;
        job.phase = "ready".into();
        write_job(&staging.join(MIGRATION_OWNER), &job)?;
        write_job(&source.join(MIGRATION_JOB), &job)?;
        rename_and_publish(&source, &destination, &staging, &mut job)
    })();
    // Keep both the stage and its journal on error. A later explicit retry
    // under the source lock can distinguish incomplete/ready/published copies.
    result.context("迁移未完成，原数据和迁移副本均已保留；请在设置中选择相同目标重试，或选择新的空目录重新迁移")
}

fn verify_copy(home: &Path, job: &MigrationJob) -> Result<()> {
    if snapshot(home, true)? != job.snapshot {
        bail!("迁移副本与源版本不一致，所有文件已保留；请选择新的空目录或联系维护者恢复");
    }
    Ok(())
}

fn publish_migration(source: &Path, destination: &Path, job: &mut MigrationJob) -> Result<Value> {
    job.phase = "published".into();
    write_job(&source.join(MIGRATION_JOB), job)?;
    private_write(&source.join(LOCATION_FILE), &serde_json::to_vec(&json!({"version":1,"data_directory":destination}))?)
        .context("完整副本已到达目标，但源盘无法写入切换指针；释放少量源盘空间后选择同一目标重试即可，无需重新复制")?;
    Ok(json!({"data_directory":destination,"source":source,"paused":true}))
}

fn rename_and_publish(
    source: &Path,
    destination: &Path,
    staging: &Path,
    job: &mut MigrationJob,
) -> Result<Value> {
    if destination.exists() {
        std::fs::remove_dir(destination)
            .context("目标目录已出现新内容，迁移已停止且完整暂存副本已保留")?;
    }
    std::fs::rename(staging, destination)?;
    #[cfg(unix)]
    std::fs::File::open(destination.parent().context("目标路径无效")?)?.sync_all()?;
    publish_migration(source, destination, job)
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
    let owner = read_job(&home.join(MIGRATION_OWNER))?;
    let source_job = read_job(&source.join(MIGRATION_JOB))?;
    if let Some(owner) = &owner {
        if owner.source != source
            || owner.destination.canonicalize()? != home.canonicalize()?
            || source_job.as_ref().is_some_and(|job| !owner.matches(job))
        {
            bail!("迁移归属记录不一致，旧位置已保留，请联系维护者");
        }
    } else if source_job.is_some() {
        bail!("新位置缺少迁移归属记录，旧位置已保留，请联系维护者");
    }
    // The relocated service is healthy and has the same credential namespace.
    // Stop tracking before workers begin, so normal indexing has no extra writes.
    remove_tracking(home)?;
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
    if source_job.is_some() {
        std::fs::remove_file(source.join(MIGRATION_JOB))?;
    }
    if owner.is_some() {
        std::fs::remove_file(home.join(MIGRATION_OWNER))?;
    }
    std::fs::remove_file(pending)?;
    Ok(())
}

#[cfg(test)]
#[path = "storage_migration_tests.rs"]
mod migration_tests;

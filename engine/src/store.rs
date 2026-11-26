use crate::types::*;
use anyhow::{bail, Context, Result};
use jieba_rs::Jieba;
use rusqlite::{params, Connection as Sqlite, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock,
    },
    time::{Duration, Instant},
};

pub struct Store {
    pub connection: Mutex<Sqlite>,
    tokenizer: Jieba,
    path: PathBuf,
    clients: RwLock<HashMap<String, (String, Connection)>>,
    pub cancelled: Arc<AtomicBool>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute::<
                *const (),
                unsafe extern "C" fn(
                    *mut rusqlite::ffi::sqlite3,
                    *mut *mut std::os::raw::c_char,
                    *const rusqlite::ffi::sqlite3_api_routines,
                ) -> std::os::raw::c_int,
            >(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
        let connection = Sqlite::open(path)?;
        let schema: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if schema > 1 {
            bail!("这个知识库由更新版本创建，请使用同版本或更新的应用打开");
        }
        connection.busy_timeout(Duration::from_secs(10))?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA wal_autocheckpoint=256;
             PRAGMA journal_size_limit=16777216;
             PRAGMA temp_store=FILE;
             PRAGMA foreign_keys=ON;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS roots(id TEXT PRIMARY KEY, path TEXT UNIQUE NOT NULL, data TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS projects(id TEXT PRIMARY KEY, path TEXT UNIQUE NOT NULL, data TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS documents(
                 id TEXT PRIMARY KEY, path TEXT UNIQUE NOT NULL, root_id TEXT NOT NULL,
                 project_id TEXT NOT NULL, version TEXT NOT NULL, status TEXT NOT NULL, data TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS documents_root ON documents(root_id);
             CREATE INDEX IF NOT EXISTS documents_project ON documents(project_id);
             CREATE TABLE IF NOT EXISTS chunks(
                 id TEXT PRIMARY KEY, document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                 ordinal INTEGER NOT NULL, text TEXT NOT NULL, location TEXT NOT NULL, lexical TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS chunks_document ON chunks(document_id, ordinal);
             CREATE VIRTUAL TABLE IF NOT EXISTS chunk_fts USING fts5(chunk_id UNINDEXED, tokens, tokenize='unicode61');
             DROP TRIGGER IF EXISTS chunks_insert;
             DROP TRIGGER IF EXISTS chunks_delete;
             DROP TRIGGER IF EXISTS chunks_insert;
             DROP TRIGGER IF EXISTS chunks_delete;
             CREATE TRIGGER chunks_insert AFTER INSERT ON chunks BEGIN
                 INSERT INTO chunk_fts(rowid,chunk_id,tokens) VALUES(new.rowid,new.id,new.lexical);
             END;
             CREATE TRIGGER chunks_delete AFTER DELETE ON chunks BEGIN
                 DELETE FROM chunk_fts WHERE rowid=old.rowid AND chunk_id=old.id;
             END;
             CREATE TABLE IF NOT EXISTS embeddings(
                 chunk_id TEXT PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
                 model TEXT NOT NULL, dimensions INTEGER NOT NULL, vector BLOB NOT NULL
             );
             CREATE TABLE IF NOT EXISTS memories(id TEXT PRIMARY KEY, data TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS connections(id TEXT PRIMARY KEY, token_hash TEXT NOT NULL, data TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS jobs(
                 path TEXT PRIMARY KEY, root_id TEXT NOT NULL, force INTEGER NOT NULL DEFAULT 0,
                 attempts INTEGER NOT NULL DEFAULT 0, available_at INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS events(
                 id INTEGER PRIMARY KEY AUTOINCREMENT, created_at TEXT NOT NULL,
                 kind TEXT NOT NULL, message TEXT NOT NULL, details TEXT NOT NULL
             );
             PRAGMA user_version=1;",
        )?;
        crate::counters::ensure(&connection, schema == 0)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let stopping = cancelled.clone();
        connection.progress_handler(2000, Some(move || stopping.load(Ordering::SeqCst)));
        let store = Self {
            connection: Mutex::new(connection),
            tokenizer: Jieba::new(),
            path: path.to_path_buf(),
            clients: RwLock::new(HashMap::new()),
            cancelled,
        };
        store.reload_clients()?;
        Ok(store)
    }

    pub fn reload_clients(&self) -> Result<()> {
        let db = self.connection.lock().unwrap();
        let mut statement = db.prepare("SELECT id,token_hash,data FROM connections")?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut clients = HashMap::new();
        for (id, hash, data) in rows {
            clients.insert(id, (hash, serde_json::from_str(&data)?));
        }
        *self.clients.write().unwrap() = clients;
        Ok(())
    }

    pub fn ensure_fts_triggers(&self) -> Result<()> {
        self.connection.lock().unwrap().execute_batch("DROP TRIGGER IF EXISTS chunks_insert; DROP TRIGGER IF EXISTS chunks_delete;
            CREATE TRIGGER chunks_insert AFTER INSERT ON chunks BEGIN INSERT INTO chunk_fts(rowid,chunk_id,tokens) VALUES(new.rowid,new.id,new.lexical); END;
            CREATE TRIGGER chunks_delete AFTER DELETE ON chunks BEGIN DELETE FROM chunk_fts WHERE rowid=old.rowid AND chunk_id=old.id; END;")?;
        Ok(())
    }

    fn reader(&self) -> Result<Sqlite> {
        self.reader_budget(2)
    }

    pub fn reader_budget(&self, seconds: u64) -> Result<Sqlite> {
        let db = Sqlite::open_with_flags(&self.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        db.busy_timeout(Duration::from_millis(100))?;
        db.execute_batch("PRAGMA query_only=ON; PRAGMA cache_size=-8192;")?;
        let start = Instant::now();
        let stopping = self.cancelled.clone();
        db.progress_handler(
            2000,
            Some(move || {
                stopping.load(Ordering::SeqCst) || start.elapsed() > Duration::from_secs(seconds)
            }),
        );
        Ok(db)
    }

    pub fn recent_documents(&self, limit: usize) -> Result<Vec<Document>> {
        let db = self.reader()?;
        let mut statement =
            db.prepare("SELECT data FROM documents ORDER BY rowid DESC LIMIT ?1")?;
        let values = statement
            .query_map([limit], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        values
            .into_iter()
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .collect()
    }

    pub fn settings(&self) -> Result<Settings> {
        Ok(self.metadata("settings")?.unwrap_or_default())
    }

    pub fn metadata<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let db = self.connection.lock().unwrap();
        let value: Option<String> = db
            .query_row("SELECT value FROM metadata WHERE key=?1", [key], |row| {
                row.get(0)
            })
            .optional()?;
        value
            .map(|text| serde_json::from_str(&text).map_err(Into::into))
            .transpose()
    }

    pub fn set_metadata<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "INSERT INTO metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }

    pub fn roots(&self) -> Result<Vec<Root>> {
        self.list_json("roots")
    }
    pub fn projects(&self) -> Result<Vec<Project>> {
        self.list_json("projects")
    }
    pub fn memories(&self) -> Result<Vec<Memory>> {
        self.list_json("memories")
    }
    pub fn connections(&self) -> Result<Vec<Connection>> {
        self.list_json("connections")
    }

    fn list_json<T: DeserializeOwned>(&self, table: &str) -> Result<Vec<T>> {
        let db = self.connection.lock().unwrap();
        let mut statement = db.prepare(&format!("SELECT data FROM {table} ORDER BY rowid"))?;
        let values = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        values
            .into_iter()
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .collect()
    }

    pub fn root(&self, id: &str) -> Result<Root> {
        self.get_json("roots", id)
    }
    pub fn project(&self, id: &str) -> Result<Project> {
        self.get_json("projects", id)
    }
    pub fn document(&self, id: &str) -> Result<Document> {
        self.get_json("documents", id)
    }
    pub fn memory(&self, id: &str) -> Result<Memory> {
        self.get_json("memories", id)
    }
    pub fn client(&self, id: &str) -> Result<Connection> {
        self.clients
            .read()
            .unwrap()
            .get(id)
            .map(|(_, client)| client.clone())
            .context("Agent 连接不存在")
    }

    fn get_json<T: DeserializeOwned>(&self, table: &str, id: &str) -> Result<T> {
        let text: String = self
            .connection
            .lock()
            .unwrap()
            .query_row(
                &format!("SELECT data FROM {table} WHERE id=?1"),
                [id],
                |row| row.get(0),
            )
            .optional()?
            .context("记录不存在或已移除")?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn save_root(&self, root: &Root) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "INSERT INTO roots(id,path,data) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET path=excluded.path,data=excluded.data",
            params![root.id,root.path,serde_json::to_string(root)?],
        )?;
        Ok(())
    }

    pub fn save_project(&self, project: &Project) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "INSERT INTO projects(id,path,data) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET path=excluded.path,data=excluded.data",
            params![project.id,project.root_path,serde_json::to_string(project)?],
        )?;
        Ok(())
    }

    pub fn ensure_project(&self, path: &str, name: &str) -> Result<Project> {
        let existing: Option<String> = self
            .connection
            .lock()
            .unwrap()
            .query_row("SELECT data FROM projects WHERE path=?1", [path], |row| {
                row.get(0)
            })
            .optional()?;
        if let Some(value) = existing {
            return Ok(serde_json::from_str(&value)?);
        }
        let project = Project {
            id: identifier(),
            name: name.into(),
            root_path: path.into(),
            goal: String::new(),
            approach: String::new(),
            decisions: vec![],
            summary_evidence: vec![],
            updated_at: timestamp(),
            summary_stale: false,
        };
        self.save_project(&project)?;
        Ok(project)
    }

    pub fn document_by_path(&self, path: &str) -> Result<Option<Document>> {
        let text: Option<String> = self
            .connection
            .lock()
            .unwrap()
            .query_row("SELECT data FROM documents WHERE path=?1", [path], |row| {
                row.get(0)
            })
            .optional()?;
        text.map(|value| serde_json::from_str(&value).map_err(Into::into))
            .transpose()
    }

    pub fn documents(&self) -> Result<Vec<Document>> {
        self.list_json("documents")
    }

    pub fn save_document(&self, document: &Document) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "INSERT INTO documents(id,path,root_id,project_id,version,status,data) VALUES(?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(id) DO UPDATE SET path=excluded.path,root_id=excluded.root_id,project_id=excluded.project_id,
             version=excluded.version,status=excluded.status,data=excluded.data",
            params![document.id,document.path,document.root_id,document.project_id,document.version,document.status,serde_json::to_string(document)?],
        )?;
        Ok(())
    }

    pub fn lexical(&self, text: &str) -> String {
        self.tokenizer
            .cut_for_search(text, true)
            .into_iter()
            .filter(|word| word.chars().any(char::is_alphanumeric))
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn replace_document(&self, document: &Document, chunks: &[Chunk]) -> Result<()> {
        let project = self.project(&document.project_id)?;
        let relative = std::path::Path::new(&document.path)
            .strip_prefix(&project.root_path)
            .unwrap_or(std::path::Path::new(&document.path))
            .to_string_lossy();
        // Tokenization is CPU work and must never hold the database write lock.
        let mut lexical = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            if self.cancelled.load(Ordering::SeqCst) {
                bail!("索引已停止");
            }
            lexical.push(self.lexical(&format!(
                "{} {} {} {}",
                project.name, relative, document.title, chunk.text
            )));
        }
        let mut db = self.connection.lock().unwrap();
        let transaction = db.transaction()?;
        transaction.execute(
            "INSERT INTO documents(id,path,root_id,project_id,version,status,data) VALUES(?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(id) DO UPDATE SET root_id=excluded.root_id,project_id=excluded.project_id,version=excluded.version,status=excluded.status,data=excluded.data",
            params![document.id,document.path,document.root_id,document.project_id,document.version,document.status,serde_json::to_string(document)?],
        )?;
        transaction.execute("DELETE FROM chunks WHERE document_id=?1", [&document.id])?;
        let maximum: i64 =
            transaction.query_row("SELECT COALESCE(MAX(rowid),0) FROM chunks", [], |row| {
                row.get(0)
            })?;
        let previous: Option<String> = transaction
            .query_row(
                "SELECT value FROM metadata WHERE key='chunk_high_water'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let mut rowid = previous
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0)
            .max(maximum);
        for (chunk, terms) in chunks.iter().zip(lexical) {
            if self.cancelled.load(Ordering::SeqCst) {
                bail!("索引已停止");
            }
            rowid += 1;
            transaction.execute("INSERT INTO chunks(rowid,id,document_id,ordinal,text,location,lexical) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![rowid,chunk.id,chunk.document_id,chunk.ordinal,chunk.text,serde_json::to_string(&chunk.location)?,terms])?;
        }
        transaction.execute("INSERT INTO metadata(key,value) VALUES('chunk_high_water',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [rowid.to_string()])?;
        // A re-index (possibly under a newly added root) supersedes an older
        // deferred removal for the same path/document id, even at the same hash.
        transaction.execute(
            "DELETE FROM cleanup_jobs WHERE document_id=?1",
            [&document.id],
        )?;
        transaction.commit()?;
        drop(db);
        self.invalidate_evidence(&document.id, Some(&document.version))?;
        Ok(())
    }

    pub fn chunks(&self, document_id: &str) -> Result<Vec<Chunk>> {
        let db = self.connection.lock().unwrap();
        let mut statement = db.prepare(
            "SELECT id,ordinal,text,location FROM chunks WHERE document_id=?1 ORDER BY ordinal",
        )?;
        let records = statement
            .query_map([document_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, usize>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        records
            .into_iter()
            .map(|(id, ordinal, text, location)| {
                Ok(Chunk {
                    id,
                    document_id: document_id.into(),
                    ordinal,
                    text,
                    location: serde_json::from_str(&location)?,
                })
            })
            .collect()
    }

    pub fn has_chunk(&self, document_id: &str, chunk_id: &str) -> bool {
        self.reader()
            .and_then(|db| {
                Ok(db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM chunks WHERE id=?1 AND document_id=?2)",
                    params![chunk_id, document_id],
                    |row| row.get::<_, bool>(0),
                )?)
            })
            .unwrap_or(false)
    }

    pub fn remove_document(&self, id: &str) -> Result<()> {
        self.schedule_cleanup_many(&[id.to_string()])?;
        while !self.remove_document_step(id)? {
            if self.cancelled.load(Ordering::SeqCst) {
                bail!("索引清理已停止");
            }
        }
        Ok(())
    }

    /// A durable tombstone hides the document before asynchronous reclamation.
    /// Keep the original version/root so a stale job cannot delete a re-index.
    pub fn schedule_cleanup_many(&self, ids: &[String]) -> Result<()> {
        let home = self.path.parent().context("知识库路径无效")?;
        for batch in ids.chunks(32) {
            self.try_checkpoint()?;
            crate::storage::require_cleanup_capacity(home, 4 * 1024 * 1024)?;
            let mut db = self.connection.lock().unwrap();
            let transaction = db.transaction()?;
            for id in batch {
                transaction.execute("INSERT INTO cleanup_jobs(document_id,version,root_id)
                    SELECT id,version,root_id FROM documents WHERE id=?1
                    ON CONFLICT(document_id) DO UPDATE SET version=excluded.version,root_id=excluded.root_id
                    WHERE cleanup_jobs.version!=excluded.version OR cleanup_jobs.root_id!=excluded.root_id", [id])?;
                transaction.execute("UPDATE documents SET status='removing',data=json_set(data,'$.status','removing','$.error','正在清理已移出资料范围的索引；原始文件不会删除') WHERE id=?1 AND status!='removing'", [id])?;
            }
            transaction.commit()?;
        }
        Ok(())
    }

    pub fn next_cleanup(&self) -> Result<Option<String>> {
        Ok(self
            .reader()?
            .query_row(
                "SELECT document_id FROM cleanup_jobs ORDER BY rowid LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn cancel_cleanup(&self, id: &str) -> Result<()> {
        let mut db = self.connection.lock().unwrap();
        let transaction = db.transaction()?;
        transaction.execute("DELETE FROM cleanup_jobs WHERE document_id=?1", [id])?;
        // A partial removal must be parsed again before it becomes searchable.
        transaction.execute("UPDATE documents SET status='indexing',data=json_set(data,'$.status','indexing','$.error','资料范围已更新，等待重新建立完整索引') WHERE id=?1 AND status='removing'", [id])?;
        transaction.commit()?;
        Ok(())
    }

    /// Do not wait for a long reader while holding the shared writer mutex.
    pub fn try_checkpoint(&self) -> Result<bool> {
        let db = self.connection.lock().unwrap();
        db.busy_timeout(Duration::ZERO)?;
        let checkpoint = db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            row.get::<_, i64>(0)
        });
        db.busy_timeout(Duration::from_secs(10))?;
        Ok(checkpoint? == 0)
    }

    /// At most 64 chunks / 4 MiB payload per rollback-safe transaction. Large
    /// legacy documents no longer cascade millions of deletes into a single WAL.
    pub fn remove_document_step(&self, id: &str) -> Result<bool> {
        self.try_checkpoint()?;
        let home = self.path.parent().context("知识库路径无效")?;
        let mut db = self.connection.lock().unwrap();
        let transaction = db.transaction()?;
        let matching = transaction.query_row("SELECT EXISTS(SELECT 1 FROM cleanup_jobs q JOIN documents d ON d.id=q.document_id
            WHERE q.document_id=?1 AND d.version=q.version AND d.root_id=q.root_id AND d.status='removing')", [id], |row| row.get::<_, bool>(0))?;
        if !matching {
            transaction.execute("DELETE FROM cleanup_jobs WHERE document_id=?1", [id])?;
            transaction.commit()?;
            return Ok(true);
        }
        let (rowids, payload) = {
            let mut statement = transaction.prepare("SELECT c.rowid,length(CAST(c.text AS BLOB))+length(CAST(c.lexical AS BLOB))+length(CAST(c.location AS BLOB))+COALESCE(length(e.vector),0)
                FROM chunks c LEFT JOIN embeddings e ON e.chunk_id=c.id WHERE c.document_id=?1 ORDER BY c.ordinal LIMIT 64")?;
            let mut ids = vec![];
            let mut payload = 0u64;
            for row in statement.query_map([id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, u64>(1)?))
            })? {
                let (rowid, bytes) = row?;
                if !ids.is_empty() && payload.saturating_add(bytes) > 4 * 1024 * 1024 {
                    break;
                }
                ids.push(rowid);
                payload = payload.saturating_add(bytes);
            }
            (ids, payload)
        };
        let finished = rowids.is_empty();
        let budget = if finished {
            1024 * 1024
        } else {
            payload.saturating_mul(4).saturating_add(16 * 1024 * 1024)
        };
        crate::storage::require_cleanup_capacity(home, budget)?;
        if finished {
            transaction.execute("DELETE FROM documents WHERE id=?1", [id])?;
        } else {
            transaction.execute("DELETE FROM chunks WHERE document_id=?1 AND rowid IN (SELECT value FROM json_each(?2))", params![id, serde_json::to_string(&rowids)?])?;
        }
        transaction.commit()?;
        drop(db);
        self.try_checkpoint()?;
        if finished {
            self.invalidate_evidence(id, None)?;
        }
        Ok(finished)
    }

    pub fn queued_paths(&self, root_id: &str) -> Result<Vec<String>> {
        let db = self.reader()?;
        let mut statement = db.prepare("SELECT path FROM jobs WHERE root_id=?1")?;
        let paths = statement
            .query_map([root_id], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(paths)
    }

    pub fn invalidate_evidence(
        &self,
        document_id: &str,
        current_version: Option<&str>,
    ) -> Result<()> {
        for mut memory in self.memories()? {
            let stale = memory.evidence.iter().any(|e| {
                e.document_id == document_id
                    && (Some(e.version.as_str()) != current_version
                        || e.chunk_id
                            .as_ref()
                            .is_some_and(|id| !self.has_chunk(document_id, id)))
            });
            if stale && memory.status != "dismissed" {
                memory.history.push(json!({"at":timestamp(),"content":memory.content,"status":memory.status,"reason":"source_changed"}));
                memory.status = "stale".into();
                memory.updated_at = timestamp();
                self.save_memory(&memory)?;
            }
        }
        for mut project in self.projects()? {
            if project.summary_evidence.iter().any(|e| {
                e.document_id == document_id
                    && (Some(e.version.as_str()) != current_version
                        || e.chunk_id
                            .as_ref()
                            .is_some_and(|id| !self.has_chunk(document_id, id)))
            }) {
                project.summary_stale = true;
                self.save_project(&project)?;
            }
        }
        Ok(())
    }

    pub fn save_memory(&self, memory: &Memory) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "INSERT INTO memories(id,data) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            params![memory.id,serde_json::to_string(memory)?],
        )?;
        Ok(())
    }

    pub fn delete_memory(&self, id: &str) -> Result<()> {
        self.connection
            .lock()
            .unwrap()
            .execute("DELETE FROM memories WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn save_client(&self, client: &Connection, token: Option<&str>) -> Result<()> {
        let db = self.connection.lock().unwrap();
        if let Some(token) = token {
            let hash = blake3::hash(token.as_bytes()).to_hex().to_string();
            db.execute(
                "INSERT INTO connections(id,token_hash,data) VALUES(?1,?2,?3)",
                params![client.id, hash, serde_json::to_string(client)?],
            )?;
        } else {
            db.execute(
                "UPDATE connections SET data=?2 WHERE id=?1",
                params![client.id, serde_json::to_string(client)?],
            )?;
        }
        let hash: String = db.query_row(
            "SELECT token_hash FROM connections WHERE id=?1",
            [&client.id],
            |row| row.get(0),
        )?;
        self.clients
            .write()
            .unwrap()
            .insert(client.id.clone(), (hash, client.clone()));
        Ok(())
    }

    pub fn touch_client(&self, id: &str) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "UPDATE connections SET data=json_set(data,'$.last_used',?2) WHERE id=?1",
            params![id, timestamp()],
        )?;
        Ok(())
    }

    pub fn authenticate(&self, id: &str, token: &str) -> Result<Connection> {
        let clients = self.clients.read().unwrap();
        let (expected, client) = clients.get(id).context("Agent 连接不存在")?;
        let digest = blake3::hash(token.as_bytes()).to_hex().to_string();
        let difference = expected
            .as_bytes()
            .iter()
            .zip(digest.as_bytes())
            .fold(0_u8, |acc, (a, b)| acc | (a ^ b));
        if expected.len() != digest.len() || difference != 0 {
            bail!("Agent 凭据无效");
        }
        if !client.enabled {
            bail!("此 Agent 连接已停用");
        }
        Ok(client.clone())
    }

    pub fn save_embedding(&self, chunk_id: &str, model: &str, vector: &[f32]) -> Result<()> {
        if vector.is_empty()
            || vector.len() > 8192
            || vector.iter().any(|v| !v.is_finite())
            || vector.iter().all(|v| *v == 0.0)
        {
            bail!("向量模型返回了无效向量");
        }
        let bytes: Vec<u8> = vector
            .iter()
            .flat_map(|number| number.to_le_bytes())
            .collect();
        self.connection.lock().unwrap().execute(
            "INSERT INTO embeddings(chunk_id,model,dimensions,vector) VALUES(?1,?2,?3,?4)
             ON CONFLICT(chunk_id) DO UPDATE SET model=excluded.model,dimensions=excluded.dimensions,vector=excluded.vector",
            params![chunk_id,model,vector.len(),bytes],
        )?;
        Ok(())
    }

    pub fn pending_embeddings(
        &self,
        model: &str,
        limit: usize,
        remote: bool,
        after: i64,
    ) -> Result<(i64, Vec<Chunk>, bool)> {
        // Seek through bounded rowid windows instead of rescanning every
        // already-embedded chunk for each eight-item batch (quadratic growth).
        let db = self.reader()?;
        let end: Option<i64> = db.query_row(
            "SELECT MAX(rowid) FROM (SELECT rowid FROM chunks WHERE rowid>?1 ORDER BY rowid LIMIT 256)",
            [after], |row|row.get(0))?;
        let Some(end) = end else {
            return Ok((after, vec![], true));
        };
        let mut statement = db.prepare(
            "SELECT c.rowid,c.id,c.document_id,c.ordinal,c.text,c.location FROM chunks c
             JOIN documents d ON d.id=c.document_id JOIN roots r ON r.id=d.root_id
             LEFT JOIN embeddings e ON e.chunk_id=c.id AND e.model=?1
             WHERE c.rowid>?4 AND c.rowid<=?5 AND e.chunk_id IS NULL AND d.status='ready'
             AND json_extract(r.data,'$.enabled')=1
             AND (?3=0 OR json_extract(r.data,'$.policy')!='local') ORDER BY c.rowid LIMIT ?2",
        )?;
        let rows = statement
            .query_map(params![model, limit, remote, after, end], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, usize>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let cursor = if rows.len() == limit {
            rows.last().unwrap().0
        } else {
            end
        };
        let chunks = rows
            .into_iter()
            .map(|(_, id, document_id, ordinal, text, location)| {
                Ok(Chunk {
                    id,
                    document_id,
                    ordinal,
                    text,
                    location: serde_json::from_str(&location)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((cursor, chunks, false))
    }

    pub fn lexical_candidates(
        &self,
        query: &str,
        limit: usize,
        roots: &[String],
        projects: &[String],
    ) -> Result<Vec<String>> {
        let mut seen = HashSet::new();
        let terms = self
            .tokenizer
            .cut_for_search(query, true)
            .into_iter()
            .filter(|word| word.chars().any(char::is_alphanumeric))
            .map(|word| word.to_lowercase())
            .filter(|word| seen.insert(word.clone()))
            .take(30)
            .map(|word| format!("\"{}\"*", word.replace('"', "\"\"")))
            .collect::<Vec<_>>();
        if terms.is_empty() {
            return Ok(vec![]);
        }
        let db = self.reader()?;
        let mut statement = db.prepare("SELECT f.chunk_id FROM chunk_fts f JOIN chunks c ON c.id=f.chunk_id JOIN documents d ON d.id=c.document_id
            WHERE chunk_fts MATCH ?1 AND d.status='ready' AND d.root_id IN (SELECT value FROM json_each(?3))
            AND d.project_id IN (SELECT value FROM json_each(?4)) ORDER BY f.rank LIMIT ?2")?;
        let ids = statement
            .query_map(
                params![
                    terms.join(" OR "),
                    limit,
                    serde_json::to_string(roots)?,
                    serde_json::to_string(projects)?
                ],
                |row| row.get(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ids)
    }

    pub fn vector_candidates(
        &self,
        model: &str,
        vector: &[f32],
        limit: usize,
        roots: &[String],
        projects: &[String],
    ) -> Result<Vec<String>> {
        let bytes: Vec<u8> = vector
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        let db = self.reader()?;
        let mut statement = db.prepare("SELECT e.chunk_id FROM embeddings e JOIN chunks c ON c.id=e.chunk_id JOIN documents d ON d.id=c.document_id
            WHERE e.model=?1 AND e.dimensions=?2 AND d.status='ready' AND d.root_id IN (SELECT value FROM json_each(?5))
            AND d.project_id IN (SELECT value FROM json_each(?6)) ORDER BY vec_distance_cosine(e.vector,?3) LIMIT ?4")?;
        let ids = statement
            .query_map(
                params![
                    model,
                    vector.len(),
                    bytes,
                    limit,
                    serde_json::to_string(roots)?,
                    serde_json::to_string(projects)?
                ],
                |row| row.get(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ids)
    }

    pub fn hit(&self, chunk_id: &str) -> Result<SearchHit> {
        let db = self.connection.lock().unwrap();
        let record: (String,String,String,String,String) = db.query_row(
            "SELECT c.id,c.text,c.location,d.data,p.data FROM chunks c JOIN documents d ON d.id=c.document_id JOIN projects p ON p.id=d.project_id WHERE c.id=?1",
            [chunk_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
        )?;
        let document: Document = serde_json::from_str(&record.3)?;
        let project: Project = serde_json::from_str(&record.4)?;
        Ok(SearchHit {
            chunk_id: record.0,
            document_id: document.id,
            title: document.title,
            project_id: document.project_id,
            project_name: project.name,
            path: document.path,
            version: document.version,
            updated_at: document.updated_at,
            text: record.1,
            location: serde_json::from_str(&record.2)?,
            score: 0.0,
        })
    }

    pub fn enqueue(&self, path: &str, root_id: &str, force: bool) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "INSERT INTO jobs(path,root_id,force) VALUES(?1,?2,?3)
             ON CONFLICT(path) DO UPDATE SET force=MAX(jobs.force,excluded.force),root_id=excluded.root_id,available_at=0,attempts=0",
            params![path,root_id,force],
        )?;
        Ok(())
    }

    pub fn next_job(&self) -> Result<Option<(String, String, bool, usize)>> {
        Ok(self.connection.lock().unwrap().query_row(
            "SELECT path,root_id,force,attempts FROM jobs WHERE available_at<=unixepoch() ORDER BY rowid LIMIT 1", [],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
        ).optional()?)
    }

    pub fn finish_job(&self, path: &str) -> Result<()> {
        self.connection
            .lock()
            .unwrap()
            .execute("DELETE FROM jobs WHERE path=?1", [path])?;
        Ok(())
    }

    pub fn retry_job(&self, path: &str, attempts: usize) -> Result<()> {
        let delay = (5_i64 * 2_i64.pow(attempts.min(9) as u32)).min(3600);
        self.connection.lock().unwrap().execute(
            "UPDATE jobs SET attempts=?2,available_at=unixepoch()+?3 WHERE path=?1",
            params![path, attempts + 1, delay],
        )?;
        Ok(())
    }

    pub fn event(&self, kind: &str, message: &str, details: Value) -> Result<()> {
        tracing::info!(kind, message, "knowledge event");
        let db = self.connection.lock().unwrap();
        db.execute(
            "INSERT INTO events(created_at,kind,message,details) VALUES(?1,?2,?3,?4)",
            params![timestamp(), kind, message, details.to_string()],
        )?;
        db.execute(
            "DELETE FROM events WHERE id < (SELECT COALESCE(MAX(id),0)-2000 FROM events)",
            [],
        )?;
        Ok(())
    }

    pub fn events(&self, limit: usize) -> Result<Vec<Event>> {
        let db = self.connection.lock().unwrap();
        let mut statement = db.prepare(
            "SELECT id,created_at,kind,message,details FROM events ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = statement
            .query_map([limit], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(id, created_at, kind, message, details)| {
                Ok(Event {
                    id,
                    created_at,
                    kind,
                    message,
                    details: serde_json::from_str(&details)?,
                })
            })
            .collect()
    }

    pub fn counts(&self) -> Result<HashMap<String, i64>> {
        let db = self.reader()?;
        let mut statement = db.prepare("SELECT name,value FROM row_counts")?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<HashMap<_, _>>>()?;
        Ok(rows)
    }

    pub fn stats(&self) -> Result<Value> {
        let counts = self.counts()?;
        let count = |name: &str| counts.get(name).copied().unwrap_or(0).max(0);
        Ok(
            json!({"documents":count("documents"),"chunks":count("chunks"),"vectors":count("embeddings"),"pending":count("jobs"),"projects":count("projects"),
            "ready":count("status:ready"),"errors":count("status:error"),"empty":count("status:empty"),"indexing":count("status:indexing"),"cleaning":count("status:removing"),"initialized":count("initialized")==1}),
        )
    }

    pub fn pending_jobs(&self) -> Result<usize> {
        Ok(self.counts()?.get("jobs").copied().unwrap_or(0).max(0) as usize)
    }
}

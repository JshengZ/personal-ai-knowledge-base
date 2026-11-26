use crate::store::Store;
use anyhow::Result;
use rusqlite::{params, Connection};
use std::collections::HashMap;

// Exact O(1) status counters: polling progress must not repeatedly scan millions
// of chunks. Legacy databases are backfilled once on a read-only snapshot.
pub fn ensure(db: &Connection, fresh: bool) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS row_counts(name TEXT PRIMARY KEY,value INTEGER NOT NULL);",
    )?;
    db.execute(
        "INSERT OR IGNORE INTO row_counts(name,value) VALUES('initialized',?1)",
        [if fresh { 1 } else { 0 }],
    )?;
    for table in ["documents", "chunks", "embeddings", "jobs", "projects"] {
        db.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS count_{table}_insert AFTER INSERT ON {table} BEGIN
            INSERT INTO row_counts(name,value) VALUES('{table}',1) ON CONFLICT(name) DO UPDATE SET value=value+1; END;
            CREATE TRIGGER IF NOT EXISTS count_{table}_delete AFTER DELETE ON {table} BEGIN
            INSERT INTO row_counts(name,value) VALUES('{table}',-1) ON CONFLICT(name) DO UPDATE SET value=value-1; END;"))?;
    }
    db.execute_batch("CREATE TRIGGER IF NOT EXISTS count_document_status_insert AFTER INSERT ON documents BEGIN
        INSERT INTO row_counts(name,value) VALUES('status:'||new.status,1) ON CONFLICT(name) DO UPDATE SET value=value+1; END;
        CREATE TRIGGER IF NOT EXISTS count_document_status_delete AFTER DELETE ON documents BEGIN
        INSERT INTO row_counts(name,value) VALUES('status:'||old.status,-1) ON CONFLICT(name) DO UPDATE SET value=value-1; END;
        CREATE TRIGGER IF NOT EXISTS count_document_status_update AFTER UPDATE OF status ON documents WHEN old.status!=new.status BEGIN
        INSERT INTO row_counts(name,value) VALUES('status:'||old.status,-1) ON CONFLICT(name) DO UPDATE SET value=value-1;
        INSERT INTO row_counts(name,value) VALUES('status:'||new.status,1) ON CONFLICT(name) DO UPDATE SET value=value+1; END;
        CREATE TRIGGER IF NOT EXISTS count_model_insert AFTER INSERT ON embeddings BEGIN
        INSERT INTO row_counts(name,value) VALUES('model:'||new.model,1) ON CONFLICT(name) DO UPDATE SET value=value+1; END;
        CREATE TRIGGER IF NOT EXISTS count_model_delete AFTER DELETE ON embeddings BEGIN
        INSERT INTO row_counts(name,value) VALUES('model:'||old.model,-1) ON CONFLICT(name) DO UPDATE SET value=value-1; END;
        CREATE TRIGGER IF NOT EXISTS count_model_update AFTER UPDATE OF model ON embeddings WHEN old.model!=new.model BEGIN
        INSERT INTO row_counts(name,value) VALUES('model:'||old.model,-1) ON CONFLICT(name) DO UPDATE SET value=value-1;
        INSERT INTO row_counts(name,value) VALUES('model:'||new.model,1) ON CONFLICT(name) DO UPDATE SET value=value+1; END;")?;
    Ok(())
}

fn read(db: &Connection) -> Result<HashMap<String, i64>> {
    let mut statement = db.prepare("SELECT name,value FROM row_counts")?;
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<HashMap<_, _>>>()?;
    Ok(rows)
}

pub fn backfill(store: &Store) -> Result<()> {
    let reader = store.reader_budget(120)?;
    reader.execute_batch("BEGIN")?;
    let baseline = read(&reader)?;
    if baseline.get("initialized") == Some(&1) {
        return Ok(());
    }
    let mut actual = HashMap::<String, i64>::new();
    for table in ["documents", "chunks", "embeddings", "jobs", "projects"] {
        actual.insert(
            table.into(),
            reader.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })?,
        );
    }
    for (table, column, prefix) in [
        ("documents", "status", "status:"),
        ("embeddings", "model", "model:"),
    ] {
        let mut statement = reader.prepare(&format!(
            "SELECT {column},COUNT(*) FROM {table} GROUP BY {column}"
        ))?;
        for row in statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })? {
            let (key, count) = row?;
            actual.insert(format!("{prefix}{key}"), count);
        }
    }
    reader.execute_batch("COMMIT")?;
    let mut writer = store.connection.lock().unwrap();
    let transaction = writer.transaction()?;
    let initialized: i64 = transaction.query_row(
        "SELECT value FROM row_counts WHERE name='initialized'",
        [],
        |row| row.get(0),
    )?;
    if initialized == 1 {
        return Ok(());
    }
    for key in baseline
        .keys()
        .chain(actual.keys())
        .collect::<std::collections::HashSet<_>>()
    {
        if key == "initialized" {
            continue;
        }
        let delta = actual.get(key).copied().unwrap_or(0) - baseline.get(key).copied().unwrap_or(0);
        transaction.execute("INSERT INTO row_counts(name,value) VALUES(?1,?2) ON CONFLICT(name) DO UPDATE SET value=value+excluded.value",params![key,delta])?;
    }
    transaction.execute("UPDATE row_counts SET value=1 WHERE name='initialized'", [])?;
    transaction.commit()?;
    tracing::info!("legacy progress counters initialized");
    Ok(())
}

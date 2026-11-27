use crate::{rules, store::Store};
use anyhow::Result;
use std::{collections::HashMap,path::Path};

// Only metadata is migrated; chunk IDs, rowids, vectors and connection tokens stay intact.
pub fn repair(store: &Store) -> Result<()> {
    let roots = store.roots()?;
    let owners: HashMap<String,String> = {
        let db = store.connection.lock().unwrap();
        let mut statement = db.prepare("SELECT DISTINCT project_id,root_id FROM documents")?;
        let values = statement.query_map([],|row|Ok((row.get(0)?,row.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        values
    };
    for mut project in store.all_projects()? {
        let owner = if let Some(id)=&project.root_id {
            roots.iter().find(|root|root.id==*id)
        } else {
            owners.get(&project.id).and_then(|id|roots.iter().find(|root|root.id==*id))
                .or_else(|| roots.iter().find(|root|rules::contains(Path::new(&root.path),Path::new(&project.root_path))))
        };
        if project.archived { continue; }
        if let Some(root) = owner {
            if project.root_id.is_none() { project.root_id=Some(root.id.clone()); store.save_project(&project)?; }
        } else { project.archived=true; store.save_project(&project)?; }
    }
    Ok(())
}

pub fn touch(store: &Store) -> Result<()> {
    let db = store.connection.lock().unwrap();
    db.execute("INSERT INTO metadata(key,value) VALUES('library_revision','1') ON CONFLICT(key) DO UPDATE SET value=CAST(CAST(metadata.value AS INTEGER)+1 AS TEXT)",[])?;
    Ok(())
}

pub fn remove(store: &Store, id: &str) -> Result<()> {
    let mut db = store.connection.lock().unwrap();
    let transaction = db.transaction()?;
    transaction.execute("UPDATE projects SET path='pkb-archive:'||id,data=json_set(data,'$.archived',json('true'),'$.summary_stale',json('true')) WHERE json_extract(data,'$.root_id')=?1",[id])?;
    transaction.execute("DELETE FROM jobs WHERE root_id=?1",[id])?;
    transaction.execute("DELETE FROM roots WHERE id=?1",[id])?;
    transaction.execute("INSERT INTO metadata(key,value) VALUES('library_revision','1') ON CONFLICT(key) DO UPDATE SET value=CAST(CAST(metadata.value AS INTEGER)+1 AS TEXT)",[])?;
    transaction.commit()?;
    drop(db);
    let active = store.projects()?.into_iter().map(|project|project.id).collect::<std::collections::HashSet<_>>();
    for mut client in store.connections()? {
        let before = client.project_ids.len();
        client.project_ids.retain(|project|active.contains(project));
        if before != client.project_ids.len() { store.save_client(&client,None)?; }
    }
    Ok(())
}

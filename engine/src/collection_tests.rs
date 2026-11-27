use crate::{collections, connections, indexer, rules, state::AppState, types::*};
use serde_json::json;
use std::path::Path;

fn root(path: &Path) -> Root {
    Root { id: identifier(), path: path.to_string_lossy().into(), name: "自定义集合".into(),
        policy:"agents".into(),enabled:true,exclusions:vec![],filters:Default::default(),created_at:timestamp(),last_scan:None,error:None }
}

#[test]
fn build_outputs_are_pruned_including_a_directly_selected_nuitka_root() {
    let temp=tempfile::tempdir().unwrap(); let source=temp.path().join("sources");
    std::fs::create_dir(&source).unwrap();let mut collection=root(&source);
    let home=temp.path().join("data");
    for name in ["tool.build","tool.dist","tool.onefile-build","APP.DIST","node_modules","build"] {
        let directory=source.join(name);std::fs::create_dir(&directory).unwrap();
        let path=directory.join("generated.c");std::fs::write(&path,"generated C").unwrap();
        let rules=rules::Rules::new(&collection,&home,Path::new("")).unwrap();
        assert!(rules.skip(&directory,true).is_some(),"{name}");assert!(rules.skip(&path,false).is_some());
    }
    let original=source.join("original.c");std::fs::write(&original,"int main(void) {return 0;}").unwrap();
    assert!(rules::Rules::new(&collection,&home,Path::new("")).unwrap().skip(&original,false).is_none());
    collection.path=source.join("tool.dist").to_string_lossy().into();
    assert!(rules::Rules::new(&collection,&home,Path::new("")).unwrap().skip(Path::new(&collection.path),true).is_some());
}

#[test]
fn explicit_exclusions_win_and_inclusion_can_restore_only_automatic_rules() {
    let temp=tempfile::tempdir().unwrap();let path=temp.path().join("tool.build");std::fs::create_dir(&path).unwrap();
    let file=path.join("keep.md");std::fs::write(&file,"keep").unwrap();
    let mut collection=root(temp.path());let home=temp.path().join("data");
    collection.filters.included_paths=vec!["tool.build/keep.md".into()];
    let rules=rules::Rules::new(&collection,&home,Path::new("")).unwrap();
    assert!(rules.skip(&path,true).is_none(),"walk ancestors of an explicitly included file");
    assert!(rules.skip(&file,false).is_none());
    collection.exclusions=vec!["tool.build".into()];
    assert!(rules::Rules::new(&collection,&home,Path::new("")).unwrap().skip(&file,false).is_some(),"parent exclusion applies to direct reads too");
    collection.exclusions.clear();collection.filters.excluded_paths=vec!["tool.build".into()];
    assert!(rules::Rules::new(&collection,&home,Path::new("")).unwrap().skip(&file,false).is_some());
    assert!(rules::validate_paths(&["../outside".into()]).is_err());
    assert!(rules::validate_paths(&["C:\\outside".into()]).is_err());
}

#[test]
fn marker_recognition_type_filters_and_binary_detection_preserve_normal_sources() {
    let temp=tempfile::tempdir().unwrap();let collection=root(temp.path());let home=temp.path().join("data");
    let generated=temp.path().join("renamed-output");std::fs::create_dir(&generated).unwrap();
    for name in ["__constants.c","__helpers.c"] {std::fs::write(generated.join(name),"generated").unwrap();}
    assert!(rules::Rules::new(&collection,&home,Path::new("")).unwrap().skip(&generated,true).unwrap().contains("Nuitka"));
    let spoofed=temp.path().join("installer.txt");std::fs::write(&spoofed,b"MZ\0\0binary").unwrap();
    assert!(rules::Rules::new(&collection,&home,Path::new("")).unwrap().skip(&spoofed,false).is_some());
    let doc=temp.path().join("document.docx");std::fs::write(&doc,b"PK\0\0Office").unwrap();
    assert!(rules::Rules::new(&collection,&home,Path::new("")).unwrap().skip(&doc,false).is_none());
    for name in ["video.mp4","archive.zip","app.exe"] {assert!(rules::Rules::new(&collection,&home,Path::new("")).unwrap().skip(&temp.path().join(name),false).is_some());}
}

#[test]
fn deleted_collections_archive_context_and_readding_the_path_creates_a_new_project() {
    let temp=tempfile::tempdir().unwrap();let source=temp.path().join("source");std::fs::create_dir(&source).unwrap();
    let state=AppState::new(temp.path().join("data"),RuntimeConfig::default()).unwrap();
    let collection=root(&source);state.store.save_root(&collection).unwrap();
    let mut project=state.store.ensure_project(&collection.path,"folder-name").unwrap();project.goal="User-authored context".into();state.store.save_project(&project).unwrap();
    assert_eq!(state.store.projects().unwrap()[0].name,collection.name);
    collections::remove(&state.store,&collection.id).unwrap();
    assert!(state.store.projects().unwrap().is_empty());
    let archived=state.store.project(&project.id).unwrap();assert!(archived.archived);assert_eq!(archived.goal,"User-authored context");
    let new_collection=root(&source);state.store.save_root(&new_collection).unwrap();
    let new_project=state.store.ensure_project(&new_collection.path,"new").unwrap();assert_ne!(new_project.id,project.id);
    collections::repair(&state.store).unwrap();assert!(state.store.project(&project.id).unwrap().archived);
}

#[test]
fn connection_defaults_are_dynamic_but_legacy_and_explicit_scopes_stay_restricted() {
    let temp=tempfile::tempdir().unwrap();let state=AppState::new(temp.path().join("data"),RuntimeConfig::default()).unwrap();
    let config=connections::create(&state,json!({"name":"automatic","kind":"other"})).unwrap();
    assert_eq!(config["connection"]["scope_mode"],"all");
    let manual=connections::create(&state,json!({"name":"restricted","project_ids":[]})).unwrap();assert_eq!(manual["connection"]["scope_mode"],"selected");
    let mut legacy=config["connection"].clone();legacy.as_object_mut().unwrap().remove("scope_mode");
    assert_eq!(serde_json::from_value::<Connection>(legacy).unwrap().scope_mode,"selected");
}

#[test]
fn changed_rules_withhold_old_vectors_and_cleanup_works_while_discovery_is_paused() {
    let temp=tempfile::tempdir().unwrap();let source=temp.path().join("sources");std::fs::create_dir(&source).unwrap();
    let output=source.join("program.build");std::fs::create_dir(&output).unwrap();let path=output.join("generated.c");std::fs::write(&path,"OLD_GENERATED_TEXT").unwrap();
    let state=AppState::new(temp.path().join("data"),RuntimeConfig::default()).unwrap();
    let mut collection=root(&source);collection.filters.automatic=false;state.store.save_root(&collection).unwrap();
    let project=state.store.ensure_project(&output.to_string_lossy(),"output").unwrap();
    let metadata=std::fs::metadata(&path).unwrap();
    let document=Document{id:identifier(),path:path.to_string_lossy().into(),title:"generated.c".into(),root_id:collection.id.clone(),project_id:project.id.clone(),extension:"c".into(),version:"v1".into(),size:metadata.len(),modified_ms:crate::state::modified_ms(&metadata),updated_at:timestamp(),status:"ready".into(),error:None,chunks:1,ocr:false};
    let chunk=Chunk{id:identifier(),document_id:document.id.clone(),ordinal:0,text:"OLD_GENERATED_TEXT".into(),location:Default::default()};
    state.store.replace_document(&document,&[chunk.clone()]).unwrap();state.store.save_embedding(&chunk.id,"old-model",&[1.,0.]).unwrap();
    collection.filters.automatic=true;state.store.save_root(&collection).unwrap();collections::touch(&state.store).unwrap();
    assert!(state.check_document(&Scope::Owner,&document,false).is_err());
    let mut settings=state.store.settings().unwrap();settings.paused=true;state.store.set_metadata("settings",&settings).unwrap();
    let mut cursor=0;let mut revision=String::new();
    assert!(indexer::discover_excluded(&state,&mut cursor,&mut revision).unwrap());
    for _ in 0..5 {if !indexer::cleanup_step(&state).unwrap(){break;}}
    assert!(state.store.document(&document.id).is_err());assert_eq!(state.store.stats().unwrap()["vectors"],0);
    assert!(path.exists());
}

fn indexed_fixture(state: &AppState, collection: &Root) -> (Document, Chunk) {
    let path=Path::new(&collection.path).join("note.md");
    std::fs::write(&path,"collection fixture").unwrap();
    let project=state.store.ensure_project(&collection.path,"disk-folder").unwrap();
    let metadata=std::fs::metadata(&path).unwrap();
    let document=Document{id:identifier(),path:path.to_string_lossy().into(),title:"note.md".into(),root_id:collection.id.clone(),project_id:project.id,extension:"md".into(),version:"v1".into(),size:metadata.len(),modified_ms:crate::state::modified_ms(&metadata),updated_at:timestamp(),status:"ready".into(),error:None,chunks:1,ocr:false};
    let chunk=Chunk{id:identifier(),document_id:document.id.clone(),ordinal:0,text:"collection fixture".into(),location:Default::default()};
    state.store.replace_document(&document,&[chunk.clone()]).unwrap();
    (document,chunk)
}

#[test]
fn final_index_commit_rechecks_deleted_and_excluded_collections_and_preserves_user_edits() {
    let temp=tempfile::tempdir().unwrap();let source=temp.path().join("sources");std::fs::create_dir(&source).unwrap();
    let state=AppState::new(temp.path().join("data"),RuntimeConfig::default()).unwrap();
    let mut collection=root(&source);state.store.save_root(&collection).unwrap();
    let (document,chunk)=indexed_fixture(&state,&collection);
    let lexical=state.store.prepare_lexical(&document,&[chunk.clone()]).unwrap();
    let mut latest=state.store.project(&document.project_id).unwrap();latest.goal="Edited during parsing".into();state.store.save_project(&latest).unwrap();
    assert!(indexer::commit_indexed(&state,&document,Some((&[chunk.clone()],&lexical))).unwrap());
    assert_eq!(state.store.project(&document.project_id).unwrap().goal,"Edited during parsing");
    collection.filters.excluded_paths=vec!["note.md".into()];state.store.save_root(&collection).unwrap();
    assert!(!indexer::commit_indexed(&state,&document,Some((&[chunk.clone()],&lexical))).unwrap());
    collections::remove(&state.store,&collection.id).unwrap();
    assert!(!indexer::commit_indexed(&state,&document,Some((&[chunk.clone()],&lexical))).unwrap());
    let archived=state.store.project(&document.project_id).unwrap();assert!(archived.archived);assert_eq!(archived.goal,"Edited during parsing");
    // An old worker cannot finish a newly queued job after the same path is re-added.
    let next=root(&source);state.store.save_root(&next).unwrap();state.store.enqueue(&document.path,&next.id,false).unwrap();
    state.store.finish_root_job(&document.path,&collection.id).unwrap();assert_eq!(state.store.next_job().unwrap().unwrap().1,next.id);
}

#[test]
fn collection_renames_are_visible_in_existing_hits_without_reindexing_or_reembedding() {
    let temp=tempfile::tempdir().unwrap();let source=temp.path().join("sources");std::fs::create_dir(&source).unwrap();
    let state=AppState::new(temp.path().join("data"),RuntimeConfig::default()).unwrap();
    let mut collection=root(&source);state.store.save_root(&collection).unwrap();
    let (_,chunk)=indexed_fixture(&state,&collection);state.store.save_embedding(&chunk.id,"fixture-model",&[1.,0.]).unwrap();
    collection.name="Renamed collection".into();state.store.save_root(&collection).unwrap();
    assert_eq!(state.store.hit(&chunk.id).unwrap().project_name,collection.name);
    assert_eq!(state.store.stats().unwrap()["vectors"],1);
}

#[test]
fn directory_links_are_excluded_on_direct_reads_as_well_as_scans() {
    let temp=tempfile::tempdir().unwrap();let source=temp.path().join("sources");std::fs::create_dir(&source).unwrap();
    let target=source.join("excluded");std::fs::create_dir(&target).unwrap();std::fs::write(target.join("note.md"),"fixture").unwrap();
    let link=source.join("nested");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target,&link).unwrap();
    #[cfg(windows)]
    assert!(std::process::Command::new("cmd").args(["/C","mklink","/J"]).arg(&link).arg(&target).output().unwrap().status.success());
    let mut collection=root(&source);collection.filters.excluded_paths=vec!["excluded".into()];
    let home=temp.path().join("data");let rules=rules::Rules::new(&collection,&home,Path::new("")).unwrap();
    assert!(rules.skip(&link.join("note.md"),false).unwrap().contains("链接"));
    assert!(rules.skip(&link,true).is_some());
    // Remove the link itself before tempfile cleanup, especially for Windows junctions.
    #[cfg(windows)]
    std::fs::remove_dir(&link).unwrap();
}

#[test]
fn legacy_collection_migration_preserves_chunk_ids_vectors_and_connection_tokens() {
    let temp=tempfile::tempdir().unwrap();let source=temp.path().join("sources");std::fs::create_dir(&source).unwrap();
    let home=temp.path().join("data");let state=AppState::new(home.clone(),RuntimeConfig::default()).unwrap();
    let collection=root(&source);state.store.save_root(&collection).unwrap();
    let (document,chunk)=indexed_fixture(&state,&collection);state.store.save_embedding(&chunk.id,"fixture-model",&[1.,0.]).unwrap();
    let config=connections::create(&state,json!({"name":"Legacy","project_ids":[document.project_id]})).unwrap();
    let id=config["connection"]["id"].as_str().unwrap().to_string();
    let token=config["server"]["env"]["PKB_CONNECTION_TOKEN"].as_str().unwrap().to_string();
    state.store.connection.lock().unwrap().execute_batch("UPDATE roots SET data=json_remove(data,'$.filters'); UPDATE projects SET data=json_remove(data,'$.root_id','$.archived'); UPDATE connections SET data=json_remove(data,'$.scope_mode'); PRAGMA user_version=1;").unwrap();
    drop(state);
    let migrated=AppState::new(home,RuntimeConfig::default()).unwrap();
    assert_eq!(migrated.store.project(&document.project_id).unwrap().root_id,Some(collection.id.clone()));
    assert!(migrated.store.root(&collection.id).unwrap().filters.automatic);
    assert_eq!(migrated.store.hit(&chunk.id).unwrap().document_id,document.id);
    assert_eq!(migrated.store.stats().unwrap()["vectors"],1);
    assert_eq!(migrated.store.client(&id).unwrap().scope_mode,"selected");
    assert!(migrated.store.authenticate(&id,&token).is_ok());
}

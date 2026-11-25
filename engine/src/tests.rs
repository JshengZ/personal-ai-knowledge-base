use crate::{
    backup, connections, retrieval,
    state::{modified_ms, AppState},
    types::*,
};
use serde_json::json;
use std::{
    io::{Cursor, Read},
    path::PathBuf,
    sync::Arc,
};

fn fixture() -> (
    tempfile::TempDir,
    Arc<AppState>,
    Root,
    Project,
    Document,
    Chunk,
) {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("knowledge");
    let sources = directory.path().join("项目");
    std::fs::create_dir_all(&sources).unwrap();
    let path = sources.join("排序实验.md");
    std::fs::write(&path, "使用 LightGBM LambdaRank 进行推荐排序实验。").unwrap();
    let metadata = std::fs::metadata(&path).unwrap();
    let state = AppState::new(home, RuntimeConfig::default()).unwrap();
    let root = Root {
        id: identifier(),
        path: sources.to_string_lossy().into_owned(),
        name: "排序项目".into(),
        policy: "agents".into(),
        enabled: true,
        exclusions: vec![],
        created_at: timestamp(),
        last_scan: None,
        error: None,
    };
    state.store.save_root(&root).unwrap();
    let project = state.store.ensure_project(&root.path, &root.name).unwrap();
    let document = Document {
        id: identifier(),
        path: path.to_string_lossy().into_owned(),
        title: "排序实验.md".into(),
        root_id: root.id.clone(),
        project_id: project.id.clone(),
        extension: "md".into(),
        version: "version-one".into(),
        size: metadata.len(),
        modified_ms: modified_ms(&metadata),
        updated_at: timestamp(),
        status: "ready".into(),
        error: None,
        chunks: 1,
        ocr: false,
    };
    let chunk = Chunk {
        id: identifier(),
        document_id: document.id.clone(),
        ordinal: 0,
        text: "使用 LightGBM LambdaRank 进行推荐排序实验。".into(),
        location: Location {
            label: "第 1 行".into(),
            line_start: Some(1),
            line_end: Some(1),
            ..Default::default()
        },
    };
    state
        .store
        .replace_document(&document, &[chunk.clone()])
        .unwrap();
    (directory, state, root, project, document, chunk)
}

#[test]
fn deleting_a_source_removes_search_rows_and_invalidates_dependent_memory() {
    let (_directory, state, root, project, document, chunk) = fixture();
    state
        .store
        .save_embedding(&chunk.id, "test-model", &[1.0, 0.0])
        .unwrap();
    let evidence = Evidence {
        document_id: document.id.clone(),
        version: document.version.clone(),
        title: document.title.clone(),
        chunk_id: Some(chunk.id.clone()),
        location: Some(chunk.location.clone()),
    };
    let memory = Memory {
        id: identifier(),
        content: "做过排序实验".into(),
        category: "experience".into(),
        status: "confirmed".into(),
        project_id: Some(project.id.clone()),
        evidence: vec![evidence],
        origin: "model".into(),
        created_at: timestamp(),
        updated_at: timestamp(),
        history: vec![],
    };
    state.store.save_memory(&memory).unwrap();
    assert!(!state
        .store
        .lexical_candidates("排序算法", 10, &[root.id.clone()], &[project.id.clone()])
        .unwrap()
        .is_empty());
    state.store.remove_document(&document.id).unwrap();
    assert!(state
        .store
        .lexical_candidates("LightGBM", 10, &[root.id.clone()], &[project.id.clone()])
        .unwrap()
        .is_empty());
    assert!(state
        .store
        .vector_candidates("test-model", &[1.0, 0.0], 10, &[root.id], &[project.id])
        .unwrap()
        .is_empty());
    assert_eq!(state.store.memory(&memory.id).unwrap().status, "stale");
}

#[test]
fn a_connection_revocation_applies_to_an_existing_scope() {
    let (_directory, state, _root, project, document, _chunk) = fixture();
    let created = connections::create(
        &state,
        json!({"name":"测试 Agent","project_ids":[project.id],"include_profile":true}),
    )
    .unwrap();
    let id = created["connection"]["id"].as_str().unwrap();
    let scope = Scope::Agent(state.store.client(id).unwrap());
    assert!(state.check_document(&scope, &document, false).is_ok());
    connections::update(&state, id, json!({"enabled":false})).unwrap();
    state.audit(&scope, "search", 0);
    assert!(!state.store.client(id).unwrap().enabled);
    assert!(state.check_document(&scope, &document, false).is_err());
    assert!(state.allowed(&scope, None, false).unwrap().0.is_empty());
}

#[test]
fn local_only_roots_hide_project_metadata_from_agents() {
    let (_directory, state, mut root, project, _document, _chunk) = fixture();
    let created = connections::create(
        &state,
        json!({"name":"测试 Agent","project_ids":[project.id]}),
    )
    .unwrap();
    let id = created["connection"]["id"].as_str().unwrap();
    let scope = Scope::Agent(state.store.client(id).unwrap());
    assert_eq!(retrieval::projects(&state, &scope).unwrap().len(), 1);
    root.policy = "local".into();
    state.store.save_root(&root).unwrap();
    assert!(retrieval::projects(&state, &scope).unwrap().is_empty());
    assert!(state
        .allowed(&Scope::Owner, None, true)
        .unwrap()
        .1
        .is_empty());
    assert_eq!(
        state.allowed(&Scope::Owner, None, false).unwrap().1.len(),
        1
    );
}

#[test]
fn vectors_are_isolated_by_model_and_dimension() {
    let (_directory, state, root, project, _document, chunk) = fixture();
    state
        .store
        .save_embedding(&chunk.id, "model-a", &[1.0, 0.0])
        .unwrap();
    assert!(state
        .store
        .vector_candidates(
            "model-b",
            &[1.0, 0.0],
            10,
            &[root.id.clone()],
            &[project.id.clone()]
        )
        .unwrap()
        .is_empty());
    assert!(state
        .store
        .vector_candidates(
            "model-a",
            &[1.0, 0.0, 0.0],
            10,
            &[root.id.clone()],
            &[project.id.clone()]
        )
        .unwrap()
        .is_empty());
    assert_eq!(
        state
            .store
            .vector_candidates("model-a", &[1.0, 0.0], 10, &[root.id], &[project.id])
            .unwrap(),
        vec![chunk.id]
    );
}

#[test]
fn zcode_install_preserves_existing_settings_and_shared_servers() {
    let (directory, state, _root, project, _document, _chunk) = fixture();
    let zcode = directory.path().join("zcode/config.json");
    let shared = directory.path().join("shared/mcp.json");
    connections::private_write(
        &zcode,
        br#"{"model":"company-model","custom":{"keep":true}}"#,
    )
    .unwrap();
    connections::private_write(
        &shared,
        br#"{"mcpServers":{"existing":{"command":"existing-server"}}}"#,
    )
    .unwrap();
    let config =
        connections::create(&state, json!({"name":"ZCode","project_ids":[project.id]})).unwrap();
    let id = config["connection"]["id"].as_str().unwrap();
    let result = connections::install_zcode_at(&state, id, &zcode, &shared, &config).unwrap();
    let updated: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&zcode).unwrap()).unwrap();
    assert_eq!(updated["model"], "company-model");
    assert_eq!(updated["custom"]["keep"], true);
    assert_eq!(
        updated["mcp"]["servers"]["existing"]["command"],
        "existing-server"
    );
    assert!(PathBuf::from(result["backup"].as_str().unwrap()).is_file());
}

#[tokio::test]
async fn backups_exclude_agent_connections_and_disable_credentials_on_restore() {
    let (_directory, state, _root, project, _document, _chunk) = fixture();
    connections::create(
        &state,
        json!({"name":"Private Agent","project_ids":[project.id]}),
    )
    .unwrap();
    let mut settings = state.store.settings().unwrap();
    settings.chat.has_key = true;
    state.store.set_metadata("settings", &settings).unwrap();
    let bytes = backup::export(&state).await.unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.clone())).unwrap();
    assert_eq!(archive.len(), 2);
    let mut snapshot = vec![];
    archive
        .by_name("knowledge.sqlite")
        .unwrap()
        .read_to_end(&mut snapshot)
        .unwrap();
    assert!(!snapshot.windows(16).any(|part| part == b"PKB_CONNECTION_T"));
    backup::restore(&state, bytes).await.unwrap();
    assert!(state.store.connections().unwrap().is_empty());
    assert!(state
        .store
        .roots()
        .unwrap()
        .iter()
        .all(|root| !root.enabled));
    assert!(!state.store.settings().unwrap().chat.has_key);
    assert!(state.store.settings().unwrap().paused);
}

#[test]
fn reading_changed_files_withholds_stale_source_text() {
    let (_directory, state, _root, _project, document, _chunk) = fixture();
    std::fs::write(&document.path, "文件被替换，新的决定已经不同。").unwrap();
    let request = retrieval::ReadRequest {
        document_id: document.id,
        chunk_id: None,
        offset: None,
        limit: None,
    };
    assert!(retrieval::read_source(&state, &Scope::Owner, &request).is_err());
}

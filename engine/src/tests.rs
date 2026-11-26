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
    let namespace=state.store.metadata::<String>("credential_namespace").unwrap();
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
    assert_eq!(state.store.metadata::<String>("credential_namespace").unwrap(),namespace);
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

#[test]
fn storage_reserves_disk_space_and_caps_index_growth() {
    use crate::storage::{policy, GIB};
    let settings = Settings::default();
    assert!(!policy(30 * GIB, GIB, 0, &settings, 64 * 1024 * 1024).blocked);
    assert!(policy(4 * GIB, GIB, 0, &settings, 0).blocked);
    assert!(policy(30 * GIB, 10 * GIB, 0, &settings, 0).blocked);
    assert!(policy(5 * GIB, GIB, 0, &settings, 64 * 1024 * 1024).blocked);
    assert!(policy(30 * GIB, 10 * GIB - 1024, 0, &settings, 2048).blocked);
}

#[test]
fn storage_migration_preserves_wal_data_ids_tokens_and_credential_namespace() {
    let (directory, state, root, project, document, chunk) = fixture();
    state
        .store
        .save_embedding(&chunk.id, "migration-vector", &[1.0, 0.0])
        .unwrap();
    let connection = connections::create(
        &state,
        json!({"name":"Migration Agent","project_ids":[project.id]}),
    )
    .unwrap();
    let id = connection["connection"]["id"].as_str().unwrap();
    let token = connections::token(&state, id).unwrap();
    let namespace: String = state
        .store
        .metadata("credential_namespace")
        .unwrap()
        .unwrap();
    let source = state.home.clone();
    std::fs::write(source.join("keep-unrecognized.txt"), "do not delete").unwrap();
    std::fs::create_dir(source.join("browser")).unwrap();
    std::fs::write(source.join("browser/cache"), "old browser cache").unwrap();
    let target = directory.path().join("new-data");
    crate::storage::migrate(&source, &target).unwrap();
    assert!(
        source.join("knowledge.sqlite").is_file(),
        "old data remains until new service starts"
    );
    let moved = AppState::new(target.clone(), RuntimeConfig::default()).unwrap();
    assert_eq!(
        moved.store.document(&document.id).unwrap().version,
        document.version
    );
    assert_eq!(connections::token(&moved, id).unwrap(), token);
    assert_eq!(
        moved
            .store
            .metadata::<String>("credential_namespace")
            .unwrap()
            .unwrap(),
        namespace
    );
    assert!(moved.store.settings().unwrap().paused);
    assert_eq!(
        moved
            .store
            .vector_candidates(
                "migration-vector",
                &[1.0, 0.0],
                10,
                &[root.id],
                &[project.id]
            )
            .unwrap(),
        vec![chunk.id]
    );
    drop(state);
    crate::storage::finish_migration(&moved.home).unwrap();
    assert!(!source.join("knowledge.sqlite").exists());
    assert!(!source.join("browser").exists());
    assert!(source.join("keep-unrecognized.txt").exists());
    assert!(
        std::path::Path::new(&document.path).is_file(),
        "original source is not moved"
    );
    assert_eq!(
        crate::storage::resolve_home(&source)
            .unwrap()
            .canonicalize()
            .unwrap(),
        target.canonicalize().unwrap()
    );
}

#[test]
fn failed_migrations_do_not_overwrite_files_or_move_live_databases() {
    use fs2::FileExt;
    let (directory, state, root, _project, document, _chunk) = fixture();
    let target = directory.path().join("occupied");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep.txt"), "existing contents").unwrap();
    assert!(crate::storage::migrate(&state.home, &target).is_err());
    assert!(crate::storage::validate_destination(
        &state.home,
        &std::path::Path::new(&root.path).join("data")
    )
    .is_err());
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .read(true)
        .create(true)
        .open(state.home.join("service.lock"))
        .unwrap();
    lock.try_lock_exclusive().unwrap();
    assert!(crate::storage::migrate(&state.home, &directory.path().join("new")).is_err());
    assert!(state.home.join("knowledge.sqlite").is_file());
    assert_eq!(state.store.document(&document.id).unwrap().id, document.id);
    assert_eq!(
        std::fs::read_to_string(target.join("keep.txt")).unwrap(),
        "existing contents"
    );
    assert!(!state.home.join(crate::storage::LOCATION_FILE).exists());
}

#[test]
fn unavailable_storage_never_falls_back_to_an_empty_old_directory() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("bootstrap");
    std::fs::create_dir(&home).unwrap();
    std::fs::write(
        home.join(crate::storage::LOCATION_FILE),
        serde_json::to_vec(
            &json!({"version":1,"data_directory":directory.path().join("missing-drive")}),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(crate::storage::resolve_home(&home).is_err());
    assert!(!home.join("knowledge.sqlite").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_mcp_initialization_and_health_bypass_a_busy_database_writer() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    let (_directory, state, _root, project, _document, _chunk) = fixture();
    let config = connections::create(
        &state,
        json!({"name":"Busy database test","project_ids":[project.id]}),
    )
    .unwrap();
    let id = config["connection"]["id"].as_str().unwrap();
    let token = config["server"]["env"]["PKB_CONNECTION_TOKEN"]
        .as_str()
        .unwrap();
    let router = crate::api::router(state.clone(), "synthetic-owner".into(), 17481);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let thread_state = state.clone();
    let thread_barrier = barrier.clone();
    let (release, wait) = std::sync::mpsc::channel();
    let held = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let thread_held = held.clone();
    let writer = std::thread::spawn(move || {
        let _lock = thread_state.store.connection.lock().unwrap();
        thread_barrier.wait();
        let _ = wait.recv_timeout(std::time::Duration::from_secs(8));
        thread_held.store(false, std::sync::atomic::Ordering::SeqCst);
    });
    barrier.wait();
    let started = std::time::Instant::now();
    let health = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header("authorization", "Bearer synthetic-owner")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    let request=Request::builder().method("POST").uri(format!("/mcp/{id}")).header("host","127.0.0.1:17481").header("authorization",format!("Bearer {token}")).header("accept","application/json, text/event-stream").header("content-type","application/json")
        .body(Body::from(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"busy-test","version":"1"}}}).to_string())).unwrap();
    let response = router.oneshot(request).await.unwrap();
    let elapsed = started.elapsed();
    let writer_still_held = held.load(std::sync::atomic::Ordering::SeqCst);
    let _ = release.send(());
    writer.join().unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert!(
        writer_still_held && elapsed < std::time::Duration::from_secs(5),
        "protocol initialization waited for the database: {elapsed:?}"
    );
}

#[test]
fn automatic_local_budget_keeps_department_api_and_explicit_cpu_modes_available() {
    let (_directory, state, _root, _project, _document, _chunk) = fixture();
    state
        .store
        .connection
        .lock()
        .unwrap()
        .execute(
            "UPDATE row_counts SET value=9174404 WHERE name='chunks'",
            [],
        )
        .unwrap();
    let mut settings = state.store.settings().unwrap();
    let automatic = crate::progress::snapshot(&state, &settings).unwrap();
    assert!(automatic.protected);
    assert_eq!(automatic.total_chunks, 9174404);
    settings.embedding_mode = "local".into();
    let local = crate::progress::snapshot(&state, &settings).unwrap();
    assert!(!local.protected);
    assert_eq!(local.mode, "local");
    assert!(local.notice.is_some());
    settings.embedding_mode = "auto".into();
    settings.embedding.base_url = "https://embedding.internal.example/v1".into();
    settings.embedding.model = "department-embedding".into();
    settings.embedding.has_key = true;
    let api = crate::progress::snapshot(&state, &settings).unwrap();
    assert!(!api.protected);
    assert_eq!(api.mode, "api");
    assert!(settings.embedding.has_key);
    settings.embedding_mode = "disabled".into();
    assert_eq!(
        crate::progress::snapshot(&state, &settings).unwrap().mode,
        "disabled"
    );
}

#[test]
fn counters_follow_reindex_model_changes_and_deletions_without_full_scans() {
    let (_directory, state, _root, _project, document, chunk) = fixture();
    assert_eq!(state.store.stats().unwrap()["chunks"], 1);
    state
        .store
        .save_embedding(&chunk.id, "model-one", &[1.0, 0.0])
        .unwrap();
    state
        .store
        .save_embedding(&chunk.id, "model-two", &[0.0, 1.0])
        .unwrap();
    let counts = state.store.counts().unwrap();
    assert_eq!(counts.get("model:model-one"), Some(&0));
    assert_eq!(counts.get("model:model-two"), Some(&1));
    state
        .store
        .replace_document(&document, &[chunk.clone()])
        .unwrap();
    assert_eq!(state.store.stats().unwrap()["vectors"], 0);
    assert_eq!(state.store.stats().unwrap()["chunks"], 1);
    let results = state
        .store
        .pending_embeddings("model-two", 32, false, 1)
        .unwrap();
    assert_eq!(
        results.1.len(),
        1,
        "reindexed chunks must receive increasing rowids"
    );
    state.store.remove_document(&document.id).unwrap();
    let stats = state.store.stats().unwrap();
    assert_eq!(stats["chunks"], 0);
    assert_eq!(stats["documents"], 0);
}

#[test]
fn project_detection_separates_unversioned_projects_and_preserves_repository_workspaces() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let first = root.join("project-a");
    let second = root.join("project-b");
    std::fs::create_dir_all(first.join("src")).unwrap();
    std::fs::create_dir_all(second.join("src")).unwrap();
    std::fs::write(first.join("package.json"), "{}").unwrap();
    std::fs::write(second.join("pyproject.toml"), "[project]\nname='example'").unwrap();
    assert_eq!(
        crate::indexer::find_project(&first.join("src/index.ts"), root),
        first
    );
    assert_eq!(
        crate::indexer::find_project(&second.join("src/config.py"), root),
        second
    );
    let repo = root.join("monorepo");
    std::fs::create_dir_all(repo.join("packages/ui/src")).unwrap();
    std::fs::write(repo.join(".git"), "gitdir: synthetic").unwrap();
    std::fs::write(repo.join("packages/ui/package.json"), "{}").unwrap();
    assert_eq!(
        crate::indexer::find_project(&repo.join("packages/ui/src/index.ts"), root),
        repo
    );
    let notes = root.join("notes");
    std::fs::create_dir(&notes).unwrap();
    assert_eq!(
        crate::indexer::find_project(&notes.join("idea.md"), root),
        notes
    );
}

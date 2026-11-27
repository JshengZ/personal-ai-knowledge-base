use crate::{
    models::Models,
    state::{modified_ms, AppState},
    types::*,
};
use serde_json::json;
use std::{path::PathBuf, sync::Arc, time::Duration};

pub(crate) fn fixture(
    count: usize,
) -> (tempfile::TempDir, Arc<AppState>, Root, Document, Vec<Chunk>) {
    let directory = tempfile::tempdir().unwrap();
    let sources = directory.path().join("sources");
    std::fs::create_dir(&sources).unwrap();
    let path = sources.join("audit.md");
    std::fs::write(&path, "synthetic review source").unwrap();
    let metadata = std::fs::metadata(&path).unwrap();
    let state = AppState::new(directory.path().join("data"), RuntimeConfig::default()).unwrap();
    let root = Root {
        id: identifier(),
        path: sources.canonicalize().unwrap().to_string_lossy().into(),
        name: "Audit".into(),
        policy: "agents".into(),
        enabled: true,
        exclusions: vec![],
        filters: Default::default(),
        created_at: timestamp(),
        last_scan: None,
        error: None,
    };
    state.store.save_root(&root).unwrap();
    let project = state.store.ensure_project(&root.path, &root.name).unwrap();
    let document = Document {
        id: identifier(),
        path: path.canonicalize().unwrap().to_string_lossy().into(),
        title: "audit.md".into(),
        root_id: root.id.clone(),
        project_id: project.id,
        extension: "md".into(),
        version: "one".into(),
        size: metadata.len(),
        modified_ms: modified_ms(&metadata),
        updated_at: timestamp(),
        status: "ready".into(),
        error: None,
        chunks: count,
        ocr: false,
    };
    let chunks = (0..count)
        .map(|ordinal| Chunk {
            id: identifier(),
            document_id: document.id.clone(),
            ordinal,
            text: format!("AUDIT_INDEX_NEEDLE fragment {ordinal}"),
            location: Location {
                label: "synthetic".into(),
                ..Default::default()
            },
        })
        .collect::<Vec<_>>();
    state.store.replace_document(&document, &chunks).unwrap();
    (directory, state, root, document, chunks)
}

#[test]
fn quota_uses_live_pages_and_keeps_physical_usage_visible() {
    let (_directory, state, _root, _document, _chunks) = fixture(1);
    {
        let db = state.store.connection.lock().unwrap();
        db.execute_batch("CREATE TABLE audit_padding(value BLOB); INSERT INTO audit_padding VALUES(zeroblob(16777216)); DROP TABLE audit_padding; PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    }
    let mut settings = state.store.settings().unwrap();
    settings.max_index_gb = 1;
    let status = crate::storage::status(
        &state.home,
        &settings,
        crate::storage::GIB - 8 * 1024 * 1024,
    )
    .unwrap();
    assert!(status.index_bytes >= 16 * 1024 * 1024);
    assert!(status.reusable_bytes >= 12 * 1024 * 1024);
    assert!(status.quota_bytes < 8 * 1024 * 1024);
    assert!(
        !status.blocked,
        "reusable SQLite pages must not permanently consume the quota"
    );
}

#[test]
fn exclusions_revoke_reads_and_schedule_cleanup_without_allowing_growth() {
    let (_directory, state, mut root, document, _chunks) = fixture(3);
    state
        .store
        .enqueue(&document.path, &root.id, false)
        .unwrap();
    root.exclusions = vec!["**/*.md".into()];
    state.store.save_root(&root).unwrap();
    assert!(
        state
            .check_document(&Scope::Owner, &document, false)
            .is_err(),
        "exclusions apply before any reconciliation work"
    );
    crate::indexer::reconcile_root(&state, &root, false, || false).unwrap();
    assert_eq!(state.store.pending_jobs().unwrap(), 0);
    assert_eq!(
        state.store.document(&document.id).unwrap().status,
        "removing"
    );
    assert_eq!(
        state.store.next_cleanup().unwrap(),
        Some(document.id.clone())
    );
    while !state.store.remove_document_step(&document.id).unwrap() {}
    assert!(state.store.document(&document.id).is_err());
    assert!(
        PathBuf::from(&document.path).is_file(),
        "cleanup never deletes the original source"
    );
}

#[test]
fn cleanup_is_bounded_persistent_and_cannot_delete_a_reindexed_document() {
    let (directory, state, root, document, chunks) = fixture(130);
    for chunk in &chunks {
        state
            .store
            .save_embedding(&chunk.id, "audit-model", &[1.0, 0.0])
            .unwrap();
    }
    state
        .store
        .schedule_cleanup_many(&[document.id.clone()])
        .unwrap();
    assert!(!state.store.remove_document_step(&document.id).unwrap());
    assert_eq!(
        state.store.chunks(&document.id).unwrap().len(),
        66,
        "one transaction removes at most 64 chunks"
    );
    assert_eq!(state.store.stats().unwrap()["vectors"], json!(66));
    let home = state.home.clone();
    drop(state);
    let state = AppState::new(home, RuntimeConfig::default()).unwrap();
    assert_eq!(
        state.store.next_cleanup().unwrap(),
        Some(document.id.clone()),
        "interrupted cleanup resumes from its durable job"
    );
    let mut replacement = document.clone();
    replacement.status = "ready".into();
    replacement.chunks = 1;
    let fresh = Chunk {
        id: identifier(),
        document_id: document.id.clone(),
        ordinal: 0,
        text: "fresh index at the same source hash".into(),
        location: Location {
            label: "fresh".into(),
            ..Default::default()
        },
    };
    state
        .store
        .replace_document(&replacement, &[fresh.clone()])
        .unwrap();
    assert!(state.store.next_cleanup().unwrap().is_none());
    assert!(state.store.remove_document_step(&document.id).unwrap());
    assert_eq!(state.store.chunks(&document.id).unwrap()[0].id, fresh.id);
    let disk = crate::storage::status(&state.home, &state.store.settings().unwrap(), 0).unwrap();
    assert!(disk.wal_bytes < 32 * 1024 * 1024);
    assert_eq!(state.store.document(&document.id).unwrap().root_id, root.id);
    drop(state);
    drop(directory);
}

#[test]
fn cleanup_stops_before_extending_a_wal_pinned_by_readers() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::File::create(directory.path().join("knowledge.sqlite-wal"))
        .unwrap()
        .set_len(32 * 1024 * 1024 + 1)
        .unwrap();
    assert!(crate::storage::require_cleanup_capacity(directory.path(), 1024 * 1024).is_err());
}

#[test]
fn paused_discovery_reclaims_confirmed_removals_but_preserves_migration_retries() {
    let (_directory, state, mut root, document, _chunks) = fixture(3);
    root.enabled = false;
    state.store.save_root(&root).unwrap();
    state
        .store
        .schedule_cleanup_many(&[document.id.clone()])
        .unwrap();
    let mut settings = state.store.settings().unwrap();
    settings.paused = true;
    state.store.set_metadata("settings", &settings).unwrap();
    assert!(!state.reconciliation_allowed());
    assert!(!state.indexing_allowed());
    assert!(
        state.cleanup_allowed(),
        "already authorized removals keep reclaiming while discovery is paused"
    );
    std::fs::write(state.home.join("migration-job.json"), "{}").unwrap();
    assert!(
        !state.cleanup_allowed(),
        "a paused interrupted migration must retain its exact source version until explicit retry"
    );
    std::fs::remove_file(state.home.join("migration-job.json")).unwrap();
    while state.store.next_cleanup().unwrap().is_some() {
        assert!(crate::indexer::cleanup_step(&state).unwrap());
    }
    assert!(
        state.store.document(&document.id).is_err(),
        "a disabled root must not cancel its removal queue"
    );
    assert!(PathBuf::from(&document.path).is_file());
    state.stop();
    assert!(!state.cleanup_allowed());
}

#[test]
fn embedding_fingerprint_tracks_the_effective_route_and_preserves_default_vectors() {
    let (_directory, state, _root, _document, _chunks) = fixture(1);
    let mut settings = state.store.settings().unwrap();
    settings.embedding.base_url = "https://models.example.test/v1".into();
    settings.embedding.model = "shared-name".into();
    let default = state.models.embedding_model(&settings);
    assert_eq!(default, "remote:https://models.example.test/v1:shared-name");
    settings.embedding.endpoint = "/v1/embeddings".into();
    assert_eq!(state.models.embedding_model(&settings), default);
    settings.embedding.endpoint = "/department-a/embed".into();
    let first = state.models.embedding_model(&settings);
    settings.embedding.endpoint = "/department-b/embed".into();
    let second = state.models.embedding_model(&settings);
    assert_ne!(first, second);
    assert_ne!(first, default);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_query_reuses_an_aged_model_that_finished_warming_while_idle() {
    // Build a small native fixture on each CI OS instead of disguising a script
    // as a Windows .exe or requiring the 800 MB model for this lifecycle test.
    let directory = tempfile::tempdir().unwrap();
    let platform = if cfg!(windows) {
        "win-x64"
    } else if cfg!(target_arch = "aarch64") {
        "mac-arm64"
    } else {
        "mac-x64"
    };
    let runtime_dir = directory.path().join("runtime").join(platform);
    let models_dir = directory.path().join("runtime/models");
    std::fs::create_dir_all(&runtime_dir).unwrap();
    std::fs::create_dir_all(&models_dir).unwrap();
    let binary = runtime_dir.join(if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    });
    let source = directory.path().join("fake_model.rs");
    std::fs::write(&source,r####"
use std::{io::{Read,Write},net::TcpListener,time::{Duration,Instant}};
fn main(){let args=std::env::args().collect::<Vec<_>>();let option=|name:&str|args[args.iter().position(|v|v==name).unwrap()+1].clone();
let log=std::path::PathBuf::from(option("--model")).with_extension("starts");let mut file=std::fs::OpenOptions::new().create(true).append(true).open(log).unwrap();writeln!(file,"{}",std::process::id()).unwrap();drop(file);
let listener=TcpListener::bind(("127.0.0.1",option("--port").parse::<u16>().unwrap())).unwrap();let start=Instant::now();
for stream in listener.incoming(){let mut stream=stream.unwrap();stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();let mut data=Vec::new();let mut buffer=[0u8;4096];loop{match stream.read(&mut buffer){Ok(0)|Err(_)=>break,Ok(n)=>data.extend_from_slice(&buffer[..n])}if let Some(end)=data.windows(4).position(|part|part==b"\r\n\r\n"){let text=String::from_utf8_lossy(&data[..end]);let length=text.lines().find_map(|line|line.split_once(':').filter(|(k,_)|k.eq_ignore_ascii_case("content-length")).and_then(|(_,v)|v.trim().parse::<usize>().ok())).unwrap_or(0);if data.len()>=end+4+length{break;}}}
let health=data.starts_with(b"GET /health");let status=if health&&start.elapsed()<Duration::from_millis(800){"503 Unavailable"}else{"200 OK"};let body=if health{"{}"}else{r##"{"data":[{"index":0,"embedding":[1.0,0.0]}]}"##};let response=format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());let _=stream.write_all(response.as_bytes());}}
"####).unwrap();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    assert!(std::process::Command::new(rustc)
        .arg("--edition=2021")
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .status()
        .unwrap()
        .success());
    let model = models_dir.join("Qwen3-Embedding-0.6B-Q8_0.gguf");
    std::fs::write(&model, "synthetic").unwrap();
    let runtime = RuntimeConfig {
        resources: directory.path().into(),
        ..Default::default()
    };
    let models = Models::new("audit-cold-model".into(), runtime, directory.path().into()).unwrap();
    let mut settings = Settings::default();
    settings.embedding_mode = "local".into();
    assert!(tokio::time::timeout(
        Duration::from_millis(100),
        models.embed(&settings, &["query".into()], true)
    )
    .await
    .is_err());
    // Native process startup can itself take more than a second. Establish
    // readiness independently without marking the manager's child as observed.
    // Only then simulate the long idle period covered by this regression.
    let port = crate::models::unobserved_model_port_for_test(&models).await;
    let client = reqwest::Client::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if client
            .get(format!("http://127.0.0.1:{port}/health"))
            .timeout(Duration::from_millis(500))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "fixture did not become healthy");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    crate::models::age_unobserved_model_for_test(&models).await;
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        models.embed(&settings, &["query".into()], true),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result, vec![vec![1.0, 0.0]]);
    assert_eq!(
        std::fs::read_to_string(model.with_extension("starts"))
            .unwrap()
            .lines()
            .count(),
        1,
        "the second query reuses the child kept alive after cancellation"
    );
    models.stop().await;
}

use super::*;
use crate::{review_regressions::fixture, state::AppState};

fn interrupted_copy(source: &Path, destination: &Path) -> MigrationJob {
    let job_id = identifier();
    install_tracking(source, &job_id).unwrap();
    let job = MigrationJob {
        version: 1,
        app: MIGRATION_APP.into(),
        job_id,
        source: source.canonicalize().unwrap(),
        destination: destination
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .join(destination.file_name().unwrap()),
        phase: "copying".into(),
        owner_pid: std::process::id(),
        created_at: timestamp(),
        snapshot: snapshot(source, true).unwrap(),
    };
    write_job(&source.join(MIGRATION_JOB), &job).unwrap();
    std::fs::create_dir(job.stage()).unwrap();
    write_job(&job.stage().join(MIGRATION_OWNER), &job).unwrap();
    job
}

fn fail_pointer(source: &Path, destination: &Path) -> MigrationJob {
    // A directory at the redirect file makes the final atomic rename fail on
    // both Windows and Unix, after the complete target has been published.
    std::fs::create_dir(source.join(LOCATION_FILE)).unwrap();
    let error = migrate(source, destination).unwrap_err();
    assert!(format!("{error:#}").contains("切换指针"));
    std::fs::remove_dir(source.join(LOCATION_FILE)).unwrap();
    read_job(&source.join(MIGRATION_JOB)).unwrap().unwrap()
}

#[test]
fn retry_discards_only_owned_incomplete_stage_after_validating_source() {
    let (directory, state, _root, document, _chunks) = fixture(1);
    let source = state.home.clone();
    drop(state);
    let destination = directory.path().join("resumed");
    let job = interrupted_copy(&source, &destination);
    std::fs::write(job.stage().join("knowledge.sqlite"), "partial copy").unwrap();
    std::fs::write(job.stage().join("partial-sentinel"), "owned stale staging").unwrap();
    migrate(&source, &destination).unwrap();
    assert!(!job.stage().exists());
    assert!(!destination.join("partial-sentinel").exists());
    let moved = AppState::new(destination.clone(), RuntimeConfig::default()).unwrap();
    assert_eq!(
        moved.store.document(&document.id).unwrap().version,
        document.version
    );
    finish_migration(&destination).unwrap();
    assert!(!source.join(MIGRATION_JOB).exists());
    assert!(!destination.join(MIGRATION_OWNER).exists());
    assert!(moved
        .store
        .metadata::<String>("migration_job_id")
        .unwrap()
        .is_none());
    let db = moved.store.connection.lock().unwrap();
    let triggers: i64 = db.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name LIKE 'migration_track_%'", [], |row| row.get(0)).unwrap();
    assert_eq!(
        triggers, 0,
        "a successful migration adds no ongoing indexing write load"
    );
}

#[test]
fn retry_preserves_unowned_stage_and_possible_sole_database_copy() {
    let (directory, state, _root, _document, _chunks) = fixture(1);
    let source = state.home.clone();
    drop(state);
    let destination = directory.path().join("resumed");
    let job = interrupted_copy(&source, &destination);
    let owner_path = job.stage().join(MIGRATION_OWNER);
    std::fs::remove_file(&owner_path).unwrap();
    std::fs::write(job.stage().join("unknown-owner-data"), "must survive").unwrap();
    assert!(migrate(&source, &destination).is_err());
    assert!(job.stage().join("unknown-owner-data").is_file());
    write_job(&owner_path, &job).unwrap();
    std::fs::copy(
        source.join("knowledge.sqlite"),
        job.stage().join("knowledge.sqlite"),
    )
    .unwrap();
    std::fs::remove_file(source.join("knowledge.sqlite")).unwrap();
    let error = migrate(&source, &destination).unwrap_err();
    assert!(format!("{error:#}").contains("已改变"));
    assert!(
        job.stage().join("knowledge.sqlite").is_file(),
        "the possible only complete copy is never removed"
    );
    assert!(!destination.exists());
}

#[test]
fn retry_reuses_published_copy_and_ignores_only_service_startup_noise() {
    let (directory, state, _root, document, _chunks) = fixture(1);
    let source = state.home.clone();
    drop(state);
    let destination = directory.path().join("resumed");
    let job = fail_pointer(&source, &destination);
    assert_eq!(job.phase, "published");
    std::fs::write(destination.join("reuse-sentinel"), "do not recopy").unwrap();
    let restarted = AppState::new(source.clone(), RuntimeConfig::default()).unwrap();
    let mut settings = restarted.store.settings().unwrap();
    settings.paused = true;
    restarted.store.set_metadata("settings", &settings).unwrap();
    restarted
        .store
        .set_metadata("http_port", &19001u16)
        .unwrap();
    restarted
        .store
        .set_metadata("last_scan", &timestamp())
        .unwrap();
    drop(restarted);
    migrate(&source, &destination).unwrap();
    assert!(destination.join("reuse-sentinel").is_file());
    assert!(!job.stage().exists());
    let moved = AppState::new(destination.clone(), RuntimeConfig::default()).unwrap();
    assert_eq!(
        moved.store.document(&document.id).unwrap().version,
        document.version
    );
    assert_eq!(
        resolve_home(&source).unwrap(),
        destination.canonicalize().unwrap()
    );
}

#[test]
fn retry_reuses_ready_stage_before_publication() {
    let (directory, state, _root, _document, _chunks) = fixture(1);
    let source = state.home.clone();
    drop(state);
    let destination = directory.path().join("resumed");
    let job = fail_pointer(&source, &destination);
    std::fs::rename(&destination, job.stage()).unwrap();
    std::fs::write(job.stage().join("ready-sentinel"), "complete staged copy").unwrap();
    migrate(&source, &destination).unwrap();
    assert!(destination.join("ready-sentinel").is_file());
    assert!(!job.stage().exists());
}

#[test]
fn changed_vectors_preserve_old_copy_and_allow_explicit_fresh_destination() {
    let (directory, state, _root, _document, chunks) = fixture(1);
    state
        .store
        .save_embedding(&chunks[0].id, "same-name", &[1.0, 0.0])
        .unwrap();
    let source = state.home.clone();
    drop(state);
    let destination = directory.path().join("old-complete");
    let job = fail_pointer(&source, &destination);
    std::fs::write(destination.join("preserved-sentinel"), "old vector space").unwrap();
    let restarted = AppState::new(source.clone(), RuntimeConfig::default()).unwrap();
    // Same model/count/dimensions, different floats: row-count-only recovery
    // fingerprints would silently roll this update back.
    restarted
        .store
        .save_embedding(&chunks[0].id, "same-name", &[0.0, 1.0])
        .unwrap();
    drop(restarted);
    let error = migrate(&source, &destination).unwrap_err();
    assert!(format!("{error:#}").contains("已改变"));
    assert!(destination.join("preserved-sentinel").is_file());
    assert!(!source.join(LOCATION_FILE).exists());
    let fresh = directory.path().join("fresh");
    migrate(&source, &fresh).unwrap();
    assert!(source
        .join(format!("migration-history-{}.json", job.job_id))
        .is_file());
    assert!(
        destination.join("knowledge.sqlite").is_file(),
        "explicit fresh migration preserves the previous complete copy"
    );
    let db = Sqlite::open_with_flags(
        fresh.join("knowledge.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let vector: Vec<u8> = db
        .query_row(
            "SELECT vector FROM embeddings WHERE chunk_id=?1",
            [&chunks[0].id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        vector,
        [0.0f32.to_le_bytes(), 1.0f32.to_le_bytes()].concat()
    );
}

#[test]
fn changed_credentials_never_activate_stale_complete_copy() {
    let (directory, state, _root, _document, _chunks) = fixture(1);
    let source = state.home.clone();
    std::fs::write(
        source.join("agent-tokens.json"),
        br#"{"synthetic":"first"}"#,
    )
    .unwrap();
    drop(state);
    let destination = directory.path().join("resumed");
    fail_pointer(&source, &destination);
    std::fs::write(
        source.join("agent-tokens.json"),
        br#"{"synthetic":"other"}"#,
    )
    .unwrap();
    assert!(migrate(&source, &destination).is_err());
    assert!(!source.join(LOCATION_FILE).exists());
    assert_eq!(
        std::fs::read(destination.join("agent-tokens.json")).unwrap(),
        br#"{"synthetic":"first"}"#
    );
}

#[test]
fn missing_journal_capacity_fails_before_creating_any_copy() {
    let (directory, state, _root, _document, _chunks) = fixture(1);
    let source = state.home.clone();
    drop(state);
    std::fs::create_dir(source.join(MIGRATION_JOB)).unwrap();
    let destination = directory.path().join("resumed");
    assert!(migrate(&source, &destination).is_err());
    assert!(!destination.exists());
    assert!(!std::fs::read_dir(directory.path())
        .unwrap()
        .any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".knowledge-migrating-")));
    assert!(source.join("knowledge.sqlite").is_file());
}

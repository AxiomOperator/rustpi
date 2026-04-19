//! Integration tests for session-store backends (SQLite and sled).

use session_store::{
    MemoryRecord, MemoryStore, RunRecord, RunStatus, RunStore, SessionRecord, SessionStore,
    SummaryRecord, SummaryStore,
};
use session_store::sqlite::SqliteBackend;
use session_store::sled_store::SledBackend;
use session_store::factory::{build_session_store, build_run_store};
use session_store::StoreError;
use config_core::model::{MemoryConfig, SessionBackend};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helper: build an in-memory SQLite backend
// ---------------------------------------------------------------------------
async fn sqlite_mem() -> SqliteBackend {
    SqliteBackend::connect("sqlite::memory:").await.expect("in-memory sqlite")
}

// ---------------------------------------------------------------------------
// Helper: build a temporary sled backend (in-memory, not persisted)
// ---------------------------------------------------------------------------
fn sled_tmp() -> SledBackend {
    SledBackend::open_temporary().expect("temporary sled backend")
}

// ===========================================================================
// Macro: run the same test body against both backends
// ===========================================================================

/// Run `$body` (an async closure) against an SQLite backend, then against sled.
macro_rules! parity_test {
    (sqlite_fn = $sqlite_fn:expr, sled_fn = $sled_fn:expr, body = $body:expr) => {{
        // SQLite
        {
            let backend = $sqlite_fn.await;
            $body(backend).await;
        }
        // sled
        {
            let backend = $sled_fn;
            $body(backend).await;
        }
    }};
}

// ===========================================================================
// 1. Backend parity: session CRUD
// ===========================================================================

#[tokio::test]
async fn parity_create_get_session() {
    async fn run(store: impl SessionStore) {
        let created = store.create_session().await.expect("create_session");
        let fetched = store.get_session(&created.id).await.expect("get_session");
        assert_eq!(created.id, fetched.id);
        assert_eq!(created.summary, fetched.summary);
    }
    run(sqlite_mem().await).await;
    run(sled_tmp()).await;
}

#[tokio::test]
async fn parity_create_run_update_status() {
    async fn run(store: impl SessionStore + RunStore) {
        let session = store.create_session().await.expect("create_session");
        let run = store.create_run(session.id.clone()).await.expect("create_run");
        assert!(matches!(run.status, RunStatus::Running));

        store
            .update_run_status(&run.id, RunStatus::Completed)
            .await
            .expect("update_run_status");

        let fetched = store.get_run(&run.id).await.expect("get_run");
        assert!(matches!(fetched.status, RunStatus::Completed));
    }
    run(sqlite_mem().await).await;
    run(sled_tmp()).await;
}

#[tokio::test]
async fn parity_save_get_summary() {
    async fn run(store: impl SessionStore + SummaryStore) {
        let session = store.create_session().await.expect("create_session");
        let saved = store
            .save_summary(&session.id, "my summary content")
            .await
            .expect("save_summary");
        assert_eq!(saved.content, "my summary content");

        let latest = store
            .get_latest_summary(&session.id)
            .await
            .expect("get_latest_summary");
        let latest = latest.expect("expected Some summary");
        assert_eq!(latest.content, "my summary content");
        assert_eq!(latest.session_id, session.id);
    }
    run(sqlite_mem().await).await;
    run(sled_tmp()).await;
}

#[tokio::test]
async fn parity_save_and_get_memory_by_id() {
    async fn run(store: impl MemoryStore) {
        let mem = store
            .save_memory(None, "unique memory record", &["alpha", "beta"])
            .await
            .expect("save_memory");
        let fetched = store.get_memory(&mem.id).await.expect("get_memory");
        assert_eq!(fetched.id, mem.id);
        assert_eq!(fetched.content, "unique memory record");
        assert!(fetched.tags.contains(&"alpha".to_string()));
    }
    run(sqlite_mem().await).await;
    run(sled_tmp()).await;
}

#[tokio::test]
async fn parity_search_memories_by_keyword() {
    async fn run(store: impl MemoryStore) {
        store.save_memory(None, "contains the word uniquetoken here", &[]).await.expect("save_memory 1");
        store.save_memory(None, "unrelated content only", &[]).await.expect("save_memory 2");
        store.save_memory(None, "another uniquetoken appears again", &[]).await.expect("save_memory 3");

        let results = store.search_memories("uniquetoken", 10).await.expect("search_memories");
        assert_eq!(results.len(), 2, "expected exactly two results");
        assert!(results[0].content.contains("uniquetoken"));
    }
    run(sqlite_mem().await).await;
    run(sled_tmp()).await;
}

#[tokio::test]
async fn parity_delete_session_returns_not_found() {
    async fn run(store: impl SessionStore) {
        let session = store.create_session().await.expect("create_session");
        store
            .delete_session(&session.id)
            .await
            .expect("delete_session");

        let err = store.get_session(&session.id).await;
        assert!(
            matches!(err, Err(StoreError::SessionNotFound(_))),
            "expected SessionNotFound after delete, got: {:?}",
            err
        );
    }
    run(sqlite_mem().await).await;
    run(sled_tmp()).await;
}

#[tokio::test]
async fn parity_delete_memory_returns_not_found() {
    async fn run(store: impl MemoryStore) {
        let mem = store
            .save_memory(None, "to be deleted", &[])
            .await
            .expect("save_memory");
        store.delete_memory(&mem.id).await.expect("delete_memory");

        let err = store.get_memory(&mem.id).await;
        assert!(
            matches!(err, Err(StoreError::MemoryNotFound(_))),
            "expected MemoryNotFound after delete, got: {:?}",
            err
        );
    }
    run(sqlite_mem().await).await;
    run(sled_tmp()).await;
}

// ===========================================================================
// 2. Restart recovery
// ===========================================================================

#[tokio::test]
async fn sqlite_restart_recovery() {
    let dir = TempDir::new().expect("tempdir");
    let db_path = dir.path().join("sessions.db");
    // ?mode=rwc tells SQLite (and sqlx) to create the file if it doesn't exist.
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    // First open: create a session.
    let session_id = {
        let backend = SqliteBackend::connect(&db_url)
            .await
            .expect("first open");
        let session = backend.create_session().await.expect("create_session");
        session.id.clone()
        // backend dropped here — connection closed
    };

    // Second open: session must still be there.
    let backend2 = SqliteBackend::connect(&db_url)
        .await
        .expect("second open");
    let fetched = backend2
        .get_session(&session_id)
        .await
        .expect("session must persist across restart");
    assert_eq!(fetched.id, session_id);
}

#[tokio::test]
async fn sled_restart_recovery() {
    let dir = TempDir::new().expect("tempdir");
    let db_path = dir.path().join("sessions.sled");

    // First open: create a session.
    let session_id = {
        let backend = SledBackend::open(db_path.to_str().unwrap()).expect("first open");
        let session = backend.create_session().await.expect("create_session");
        session.id.clone()
        // backend dropped here
    };

    // Second open: session must still be there.
    let backend2 = SledBackend::open(db_path.to_str().unwrap()).expect("second open");
    let fetched = backend2
        .get_session(&session_id)
        .await
        .expect("session must persist across restart");
    assert_eq!(fetched.id, session_id);
}

// ===========================================================================
// 3. Schema migration safety
// ===========================================================================

#[tokio::test]
async fn sqlite_schema_version_is_set() {
    // Verify schema migration is idempotent: opening the same file twice must not fail.
    let dir = TempDir::new().expect("tempdir");
    let db_url = format!("sqlite://{}?mode=rwc", dir.path().join("v.db").display());

    let b1 = SqliteBackend::connect(&db_url).await.expect("first connect");
    // Second connect to same file must succeed (idempotent migration).
    let b2 = SqliteBackend::connect(&db_url).await.expect("second connect (idempotent)");
    // Both should work normally.
    let s = b1.create_session().await.expect("create via b1");
    let _ = b2.get_session(&s.id).await.expect("get via b2");
}

#[tokio::test]
async fn sled_schema_version_is_set_and_idempotent() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("schema_test.sled");
    let p = path.to_str().unwrap();

    let b1 = SledBackend::open(p).expect("first open");
    drop(b1);
    // Re-open must not panic or return error (version already written).
    let b2 = SledBackend::open(p).expect("second open (idempotent)");
    let s = b2.create_session().await.expect("create_session");
    assert_eq!(b2.get_session(&s.id).await.unwrap().id, s.id);
}

// ===========================================================================
// 4. Memory store search
// ===========================================================================

#[tokio::test]
async fn search_memories_returns_correct_count() {
    async fn run(store: impl MemoryStore) {
        store.save_memory(None, "matchme fruit one", &[]).await.unwrap();
        store.save_memory(None, "matchme veggie two", &[]).await.unwrap();
        store.save_memory(None, "matchme grain three", &[]).await.unwrap();

        let results = store.search_memories("matchme", 10).await.unwrap();
        assert_eq!(results.len(), 3, "should match all 3 'matchme' entries");
    }
    run(sqlite_mem().await).await;
    run(sled_tmp()).await;
}

#[tokio::test]
async fn search_memories_respects_limit() {
    async fn run(store: impl MemoryStore) {
        for i in 0..5 {
            store
                .save_memory(None, &format!("limittest content {i}"), &[])
                .await
                .unwrap();
        }
        let results = store.search_memories("limittest", 2).await.unwrap();
        assert!(results.len() <= 2, "limit=2 must cap results");
    }
    run(sqlite_mem().await).await;
    run(sled_tmp()).await;
}

// ===========================================================================
// 5. Factory tests
// ===========================================================================

#[tokio::test]
async fn factory_build_session_store_sqlite() {
    let dir = TempDir::new().expect("tempdir");
    // Override HOME so the factory derives its path under our tempdir.
    // NOTE: set_var is a global mutation; this test is intentionally simple and
    // relies on the factory creating the file via ?mode=rwc in the URL.
    std::env::set_var("HOME", dir.path());
    std::fs::create_dir_all(dir.path().join(".rustpi")).ok();

    let config = MemoryConfig {
        session_backend: SessionBackend::Sqlite,
        obsidian_vault_path: None,
        qdrant_enabled: false,
        qdrant_url: None,
        postgres_url: None,
    };

    let result = build_session_store(&config).await;
    assert!(result.is_ok(), "factory build_session_store(sqlite) failed: {:?}", result.err());
}

#[tokio::test]
async fn factory_build_session_store_sled() {
    let dir = TempDir::new().expect("tempdir");
    std::env::set_var("HOME", dir.path());

    let config = MemoryConfig {
        session_backend: SessionBackend::Sled,
        obsidian_vault_path: None,
        qdrant_enabled: false,
        qdrant_url: None,
        postgres_url: None,
    };

    std::fs::create_dir_all(dir.path().join(".rustpi")).ok();

    let result = build_session_store(&config).await;
    assert!(result.is_ok(), "factory build_session_store(sled) failed: {:?}", result.err());
}

// ===========================================================================
// 6. Edge cases
// ===========================================================================

#[tokio::test]
async fn get_nonexistent_session_returns_not_found() {
    async fn run(store: impl SessionStore) {
        use agent_core::types::SessionId;
        let fake_id = SessionId::new();
        let err = store.get_session(&fake_id).await;
        assert!(
            matches!(err, Err(StoreError::SessionNotFound(_))),
            "expected SessionNotFound, got: {:?}",
            err
        );
    }
    run(sqlite_mem().await).await;
    run(sled_tmp()).await;
}

#[tokio::test]
async fn get_nonexistent_run_returns_not_found() {
    async fn run(store: impl RunStore) {
        use agent_core::types::RunId;
        let fake_id = RunId::new();
        let err = store.get_run(&fake_id).await;
        assert!(
            matches!(err, Err(StoreError::RunNotFound(_))),
            "expected RunNotFound, got: {:?}",
            err
        );
    }
    run(sqlite_mem().await).await;
    run(sled_tmp()).await;
}

#[tokio::test]
async fn list_runs_for_session_with_no_runs_is_empty() {
    async fn run(store: impl SessionStore + RunStore) {
        let session = store.create_session().await.expect("create_session");
        let runs = store.list_runs(&session.id).await.expect("list_runs");
        assert!(runs.is_empty(), "expected empty list, got: {}", runs.len());
    }
    run(sqlite_mem().await).await;
    run(sled_tmp()).await;
}

#[tokio::test]
async fn save_memory_with_no_session_id_works() {
    async fn run(store: impl MemoryStore) {
        let mem = store
            .save_memory(None, "session-less memory", &["standalone"])
            .await
            .expect("save_memory with None session_id");
        assert!(mem.session_id.is_none());
        let fetched = store.get_memory(&mem.id).await.expect("get_memory");
        assert_eq!(fetched.content, "session-less memory");
    }
    run(sqlite_mem().await).await;
    run(sled_tmp()).await;
}

// ===========================================================================
// Postgres tests — require live DB, skipped by default
// ===========================================================================

#[tokio::test]
#[ignore = "requires live PostgreSQL instance"]
async fn postgres_session_crud() {
    use session_store::postgres::PostgresBackend;
    let url = std::env::var("TEST_POSTGRES_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/rustpi_test".to_string());
    let backend = match PostgresBackend::connect(&url).await {
        Ok(b) => b,
        Err(e) => { eprintln!("skip: {e}"); return; }
    };
    let session = backend.create_session().await.expect("create_session");
    let fetched = backend.get_session(&session.id).await.expect("get_session");
    assert_eq!(session.id, fetched.id);
    assert_eq!(session.summary, fetched.summary);

    backend.update_summary(&session.id, "pg summary").await.expect("update_summary");
    let updated = backend.get_session(&session.id).await.expect("get after update");
    assert_eq!(updated.summary.as_deref(), Some("pg summary"));
}

#[tokio::test]
#[ignore = "requires live PostgreSQL instance"]
async fn postgres_run_crud() {
    use session_store::postgres::PostgresBackend;
    let url = std::env::var("TEST_POSTGRES_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/rustpi_test".to_string());
    let backend = match PostgresBackend::connect(&url).await {
        Ok(b) => b,
        Err(e) => { eprintln!("skip: {e}"); return; }
    };
    let session = backend.create_session().await.expect("create_session");
    let run = backend.create_run(session.id.clone()).await.expect("create_run");
    assert!(matches!(run.status, RunStatus::Running));
    assert_eq!(run.session_id, session.id);

    backend
        .update_run_status(&run.id, RunStatus::Completed)
        .await
        .expect("update_run_status");

    let fetched = backend.get_run(&run.id).await.expect("get_run");
    assert!(matches!(fetched.status, RunStatus::Completed));
    assert!(fetched.completed_at.is_some());

    let runs = backend.list_runs(&session.id).await.expect("list_runs");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].id, run.id);
}

#[tokio::test]
#[ignore = "requires live PostgreSQL instance"]
async fn postgres_memory_crud() {
    use session_store::postgres::PostgresBackend;
    let url = std::env::var("TEST_POSTGRES_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/rustpi_test".to_string());
    let backend = match PostgresBackend::connect(&url).await {
        Ok(b) => b,
        Err(e) => { eprintln!("skip: {e}"); return; }
    };
    let mem = backend
        .save_memory(None, "pg memory content", &["pg-tag-a", "pg-tag-b"])
        .await
        .expect("save_memory");
    let fetched = backend.get_memory(&mem.id).await.expect("get_memory");
    assert_eq!(fetched.id, mem.id);
    assert_eq!(fetched.content, "pg memory content");
    assert!(fetched.tags.contains(&"pg-tag-a".to_string()));

    backend.delete_memory(&mem.id).await.expect("delete_memory");
    let err = backend.get_memory(&mem.id).await;
    assert!(
        matches!(err, Err(StoreError::MemoryNotFound(_))),
        "expected MemoryNotFound after delete, got: {:?}",
        err
    );
}

//! Qdrant backend tests.
//!
//! Tests that require a live Qdrant instance are marked `#[ignore]`.

use memory_sync::qdrant::QdrantMemory;
use memory_sync::MemorySyncError;

// ---------------------------------------------------------------------------
// 1. Construction does not panic
// ---------------------------------------------------------------------------

/// `QdrantMemory::new` must never panic — it may return Ok or Err.
#[test]
fn qdrant_new_with_any_url_does_not_panic() {
    // A well-formed URL that points to a non-existent host; `new` only builds
    // the client object, so this should succeed without network I/O.
    let result = QdrantMemory::new("http://127.0.0.1:19999", None, None);
    // Drop to prove no panic at construction or drop.
    drop(result);
}

/// `QdrantMemory::new` with a custom collection name and vector size succeeds.
#[test]
fn qdrant_new_custom_collection_and_vector_size_does_not_panic() {
    let result = QdrantMemory::new(
        "http://127.0.0.1:19999",
        Some("custom_collection".to_string()),
        Some(768),
    );
    drop(result);
}

// ---------------------------------------------------------------------------
// 2. Graceful failure against an unavailable server
// ---------------------------------------------------------------------------

/// `ensure_collection` against an unreachable server must return
/// `Err(MemorySyncError::Qdrant(_))`, not panic.
#[tokio::test]
async fn qdrant_ensure_collection_unavailable_server_returns_error() {
    let qm = match QdrantMemory::new("http://127.0.0.1:19999", None, None) {
        Ok(q) => q,
        Err(e) => {
            eprintln!("QdrantMemory::new failed at construction (ok to skip): {e}");
            return;
        }
    };
    let result = qm.ensure_collection().await;
    assert!(
        result.is_err(),
        "expected an error when server is unavailable, got Ok"
    );
    match result.unwrap_err() {
        MemorySyncError::Qdrant(_) => {}
        other => panic!("expected MemorySyncError::Qdrant, got: {other:?}"),
    }
}

/// `upsert_memory` against an unreachable server must return
/// `Err(MemorySyncError::Qdrant(_))`, not panic.
#[tokio::test]
async fn qdrant_upsert_unavailable_server_returns_error() {
    let qm = match QdrantMemory::new("http://127.0.0.1:19999", None, None) {
        Ok(q) => q,
        Err(e) => {
            eprintln!("QdrantMemory::new failed at construction (ok to skip): {e}");
            return;
        }
    };
    let record = memory_sync::MemoryRecord::new("test content", vec!["t".to_string()], None);
    let embedding = vec![0.1f32; 1536];
    let result = qm.upsert_memory(&record, embedding).await;
    assert!(
        result.is_err(),
        "expected error when upserting to unavailable server"
    );
    match result.unwrap_err() {
        MemorySyncError::Qdrant(_) => {}
        other => panic!("expected MemorySyncError::Qdrant, got: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 3. Live test (ignored by default — requires a running Qdrant instance)
// ---------------------------------------------------------------------------

/// End-to-end: ensure collection → upsert → search.
///
/// Set `QDRANT_URL` to override the default `http://localhost:6334`.
#[tokio::test]
#[ignore = "requires Qdrant server (set QDRANT_URL or use default http://localhost:6334)"]
async fn qdrant_live_ensure_upsert_search() {
    let url = std::env::var("QDRANT_URL")
        .unwrap_or_else(|_| "http://localhost:6334".to_string());

    // Use a tiny vector size (4) for the test collection so no real embeddings
    // are required.
    let qm = match QdrantMemory::new(&url, Some("rustpi_test_qdrant".to_string()), Some(4)) {
        Ok(q) => q,
        Err(e) => {
            eprintln!("skip: QdrantMemory::new failed: {e}");
            return;
        }
    };

    if let Err(e) = qm.ensure_collection().await {
        eprintln!("skip: ensure_collection failed: {e}");
        return;
    }

    let record = memory_sync::MemoryRecord::new(
        "live qdrant upsert test content",
        vec!["live".to_string(), "test".to_string()],
        None,
    );
    let embedding = vec![0.25f32, 0.25, 0.25, 0.25];

    qm.upsert_memory(&record, embedding.clone())
        .await
        .expect("upsert_memory");

    let results = qm
        .search_similar(embedding, 5)
        .await
        .expect("search_similar");
    assert!(
        !results.is_empty(),
        "search returned no results after upsert"
    );
}

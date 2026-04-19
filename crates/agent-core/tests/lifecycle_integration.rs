//! End-to-end lifecycle integration tests for agent-core.
//!
//! These tests exercise the real run lifecycle using actual in-process components:
//! - Session / Run state machines (agent-core)
//! - EventBus fan-out and durable log (agent-core)
//! - SQLite session/run persistence (session-store)
//! - Context assembly on a real temp directory (context-engine)
//! - Tool execution with event emission (tool-runtime)

// ---------------------------------------------------------------------------
// Test 1: Session creation + Run state transitions + SQLite persistence
// ---------------------------------------------------------------------------

/// Drives a full run through Created → Queued → Running → Completed,
/// persisting session and run records to an in-memory SQLite backend, and
/// verifying that the EventBus recorded all expected lifecycle events.
#[tokio::test]
async fn session_create_and_run_state_transitions() {
    use agent_core::{
        bus::EventBus,
        run::{RunManager, RunParams, RunStatus},
        session::SessionManager,
        types::{AgentEvent, ModelId, ProviderId},
    };
    use session_store::{
        sqlite::SqliteBackend, RunStatus as StoreRunStatus, RunStore, SessionStore,
    };

    // ── In-process state ──────────────────────────────────────────────────
    let bus = EventBus::new();
    let mut sessions = SessionManager::new();
    let mut runs = RunManager::new();

    // Create session.
    let (session_id, sess_created_ev) = sessions.create();
    bus.emit(sess_created_ev);

    // Attach a new run.
    let (run_id, run_created_ev) = runs.create(session_id.clone());
    bus.emit(run_created_ev);
    sessions
        .get_mut(&session_id)
        .unwrap()
        .attach_run(run_id.clone())
        .unwrap();

    // Queue → Running → Completed.
    let run = runs.get_mut(&run_id).unwrap();
    let ev = run.queue().unwrap();
    bus.emit(ev);

    let params = RunParams {
        session_id: session_id.clone(),
        provider: ProviderId::new("mock"),
        model: ModelId::new("mock-model"),
    };
    let ev = run.start(params).unwrap();
    bus.emit(ev);
    assert_eq!(run.status, RunStatus::Running);

    let ev = run.complete().unwrap();
    bus.emit(ev);
    assert_eq!(run.status, RunStatus::Completed);
    assert!(run.status.is_terminal());

    // ── Persistence (SQLite in-memory) ────────────────────────────────────
    let store = SqliteBackend::connect("sqlite::memory:").await.unwrap();
    let sess_rec = store.create_session().await.unwrap();
    let run_rec = store.create_run(sess_rec.id.clone()).await.unwrap();
    store
        .update_run_status(&run_rec.id, StoreRunStatus::Completed)
        .await
        .unwrap();
    let fetched = store.get_run(&run_rec.id).await.unwrap();
    assert!(matches!(fetched.status, StoreRunStatus::Completed));

    // ── EventBus audit ────────────────────────────────────────────────────
    let events = bus.all_events();
    // We emitted: SessionCreated, RunCreated, RunQueued, RunStarted, RunCompleted.
    assert_eq!(events.len(), 5);
    assert!(matches!(events[0], AgentEvent::SessionCreated { .. }));
    assert!(matches!(events[1], AgentEvent::RunCreated { .. }));
    assert!(matches!(events[2], AgentEvent::RunQueued { .. }));
    assert!(matches!(events[3], AgentEvent::RunStarted { .. }));
    assert!(matches!(events[4], AgentEvent::RunCompleted { .. }));

    // events_for_run should return all run-scoped events.
    let run_events = bus.events_for_run(&run_id);
    assert!(
        run_events
            .iter()
            .any(|e| matches!(e, AgentEvent::RunStarted { .. })),
        "RunStarted must be present in run-scoped events"
    );
    assert!(
        run_events
            .iter()
            .any(|e| matches!(e, AgentEvent::RunCompleted { .. })),
        "RunCompleted must be present in run-scoped events"
    );
}

// ---------------------------------------------------------------------------
// Test 2: ContextEngine builds a bounded context for a real temp project
// ---------------------------------------------------------------------------

/// Verifies that the ContextEngine:
/// - Scans a real temp directory
/// - Respects .gitignore (excluded files absent from output)
/// - Produces a non-empty PackedContext within the token budget
/// - Includes at least one Rust source file
#[tokio::test]
async fn context_engine_builds_bounded_context_for_temp_project() {
    use context_engine::{ContextEngine, EngineConfig, RelevanceHints};
    use std::fs;
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    // Minimal Cargo workspace.
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "test-project"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();

    // A relevant source file.
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/main.rs"),
        r#"fn main() {
    println!("hello from test project");
}
"#,
    )
    .unwrap();

    // A library file.
    fs::write(
        root.join("src/lib.rs"),
        r#"pub fn add(a: i32, b: i32) -> i32 { a + b }
"#,
    )
    .unwrap();

    // A file that should be ignored.
    fs::write(root.join(".gitignore"), "ignored_file.txt\n").unwrap();
    fs::write(root.join("ignored_file.txt"), "this must not appear in context").unwrap();

    // Build context with a reasonable token budget.
    let config = EngineConfig {
        project_root: root.to_path_buf(),
        token_budget: 8_000,
        memory_budget: 0,
        max_scan_files: 100,
        max_workset_files: 20,
        min_relevance_score: 0.0,
        max_per_dir: 10,
        compaction_threshold: 1.5,
    };

    let engine = ContextEngine::new(config);
    let hints = RelevanceHints::default();
    let (packed, stats) = engine.build_context(hints, None).await.unwrap();

    // Token budget must be respected.
    assert!(
        packed.total_tokens <= 8_000,
        "token budget exceeded: {} > 8000",
        packed.total_tokens
    );

    // At least one file must be selected.
    assert!(
        !packed.blocks.is_empty(),
        "expected at least one file block, got none"
    );
    assert!(
        stats.files_selected >= 1,
        "stats.files_selected should be >= 1"
    );

    // The ignored file must not appear in the output.
    let rendered = packed.render();
    assert!(
        !rendered.contains("this must not appear in context"),
        "ignored_file.txt content leaked into context"
    );

    // At least one Rust source file should be present.
    let has_rust = packed.blocks.iter().any(|b| {
        b.path.extension().map(|e| e == "rs").unwrap_or(false)
    });
    assert!(has_rust, "expected at least one .rs file in packed context");
}

// ---------------------------------------------------------------------------
// Test 3: EventBus delivers events to multiple subscribers in order
// ---------------------------------------------------------------------------

/// Subscribes two independent receivers before emitting 3 events and verifies
/// that each receiver independently receives all 3 events in emission order.
#[tokio::test]
async fn event_bus_delivers_events_to_subscribers() {
    use agent_core::{
        bus::EventBus,
        types::{AgentEvent, RunId, SessionId},
    };

    let bus = EventBus::new();
    let mut rx1 = bus.subscribe();
    let mut rx2 = bus.subscribe();

    let sid = SessionId::new();
    let rid = RunId::new();

    let e1 = AgentEvent::SessionCreated {
        session_id: sid.clone(),
        timestamp: chrono::Utc::now(),
    };
    let e2 = AgentEvent::RunCreated {
        run_id: rid.clone(),
        session_id: sid.clone(),
        timestamp: chrono::Utc::now(),
    };
    let e3 = AgentEvent::RunCompleted {
        run_id: rid.clone(),
        timestamp: chrono::Utc::now(),
    };

    bus.emit(e1);
    bus.emit(e2);
    bus.emit(e3);

    // Both subscribers must receive all 3 events in order.
    for rx in [&mut rx1, &mut rx2] {
        let got1 = rx.try_recv().unwrap();
        let got2 = rx.try_recv().unwrap();
        let got3 = rx.try_recv().unwrap();
        assert!(matches!(got1, AgentEvent::SessionCreated { .. }));
        assert!(matches!(got2, AgentEvent::RunCreated { .. }));
        assert!(matches!(got3, AgentEvent::RunCompleted { .. }));
    }

    // Durable log also contains all 3.
    assert_eq!(bus.len(), 3);
}

// ---------------------------------------------------------------------------
// Test 4: Run cancellation emits CancellationRequested + RunCancelled
// ---------------------------------------------------------------------------

/// Cancels a run mid-execution and verifies that both CancellationRequested
/// and RunCancelled events are emitted to the bus and the run state is Cancelled.
#[tokio::test]
async fn run_cancellation_emits_cancelled_event() {
    use agent_core::{
        bus::EventBus,
        run::{RunManager, RunParams, RunStatus},
        session::SessionManager,
        types::{AgentEvent, ModelId, ProviderId},
    };

    let bus = EventBus::new();
    let mut sessions = SessionManager::new();
    let mut runs = RunManager::new();

    let (session_id, sess_ev) = sessions.create();
    bus.emit(sess_ev);

    let (run_id, run_created_ev) = runs.create(session_id.clone());
    bus.emit(run_created_ev);

    // Queue and start the run.
    let run = runs.get_mut(&run_id).unwrap();
    let ev = run.queue().unwrap();
    bus.emit(ev);

    let params = RunParams {
        session_id: session_id.clone(),
        provider: ProviderId::new("mock"),
        model: ModelId::new("mock-model"),
    };
    let ev = run.start(params).unwrap();
    bus.emit(ev);
    assert_eq!(run.status, RunStatus::Running);

    // Cancel the run.
    let (req_ev, cancel_ev) = run.cancel().unwrap();
    bus.emit(req_ev);
    bus.emit(cancel_ev);

    // State must be Cancelled and terminal.
    assert_eq!(run.status, RunStatus::Cancelled);
    assert!(run.status.is_terminal());
    assert!(run.cancel_token.is_cancelled());

    // Verify both cancellation events are in the bus.
    let run_events = bus.events_for_run(&run_id);
    assert!(
        run_events
            .iter()
            .any(|e| matches!(e, AgentEvent::CancellationRequested { .. })),
        "CancellationRequested must be emitted"
    );
    assert!(
        run_events
            .iter()
            .any(|e| matches!(e, AgentEvent::RunCancelled { .. })),
        "RunCancelled must be emitted"
    );

    // Attempting to cancel again must fail.
    assert!(
        run.cancel().is_err(),
        "double-cancel should return an error"
    );
}

// ---------------------------------------------------------------------------
// Test 5: ToolRunner executes read_file and emits ToolStarted/ToolCompleted
// ---------------------------------------------------------------------------

/// Sets up a ToolRunner with a ReadFileTool (connected to the EventBus broadcast
/// channel), writes a known file to a temp directory, executes the tool, and
/// verifies that the result contains the expected content and that ToolStarted +
/// ToolCompleted events were broadcast.
#[tokio::test]
async fn tool_runner_executes_read_file_and_records_event() {
    use agent_core::{
        bus::EventBus,
        types::{AgentEvent, RunId, ToolCall},
    };
    use std::fs;
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;
    use tool_runtime::{
        path_safety::PathSafetyPolicy,
        registry::ToolRegistry,
        runner::ToolRunner,
        schema::ToolConfig,
        tools::file::ReadFileTool,
    };

    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("hello.txt");
    fs::write(&file_path, "hello from tool test").unwrap();

    // Build a policy that allows the temp dir.
    let policy = Arc::new(PathSafetyPolicy::new([tmp.path()]));
    let read_tool = Arc::new(ReadFileTool::new(policy));

    let mut registry = ToolRegistry::default();
    registry.register(read_tool);
    let registry = Arc::new(registry);

    // Create a dedicated broadcast channel for tool lifecycle events.
    let (ev_tx, mut ev_rx) = tokio::sync::broadcast::channel::<AgentEvent>(64);
    let runner = ToolRunner::new(registry, Duration::from_secs(10))
        .with_event_tx(ev_tx);

    let run_id = RunId::new();
    let call = ToolCall {
        id: "call-1".to_string(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({ "path": file_path.to_str().unwrap() }),
    };

    let config = ToolConfig {
        run_id: Some(run_id.clone()),
        ..Default::default()
    };

    let result = runner.execute(call, config).await.unwrap();

    // The tool must return success with file content.
    assert!(result.success, "read_file must succeed");
    let content = result.output["content"].as_str().unwrap_or("");
    assert!(
        content.contains("hello from tool test"),
        "file content missing from tool result; got: {:?}",
        result.output
    );

    // Verify ToolStarted and ToolCompleted were broadcast.
    let mut got_started = false;
    let mut got_completed = false;
    while let Ok(ev) = ev_rx.try_recv() {
        match &ev {
            AgentEvent::ToolStarted { run_id: r, tool_name, .. } if r == &run_id && tool_name == "read_file" => {
                got_started = true;
            }
            AgentEvent::ToolCompleted { run_id: r, tool_name, .. } if r == &run_id && tool_name == "read_file" => {
                got_completed = true;
            }
            _ => {}
        }
    }
    assert!(got_started, "ToolStarted event must be emitted");
    assert!(got_completed, "ToolCompleted event must be emitted");
}

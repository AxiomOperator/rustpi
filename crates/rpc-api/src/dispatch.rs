//! Method handlers for each `RpcMethod` variant.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use agent_core::run::{Run, RunParams};
use agent_core::session::Session;
use agent_core::session::SessionStatus;
use agent_core::types::{AgentEvent, ModelId, ProviderId, RunId, SessionId};
use chrono::Utc;
use serde_json::json;

use crate::error::RpcError;
use crate::normalize::normalize_event;
use crate::protocol::{
    AuthStatusInfo, CapabilitiesInfo, RpcErrorCode, RpcRequest, RpcResponse, RunInfo, SessionInfo,
};
use crate::server::ServerState;
use crate::transport::LineWriter;

/// Route an `RpcRequest` to the appropriate handler.
pub async fn dispatch<W>(
    request: &RpcRequest,
    state: &Arc<ServerState>,
    writer: &LineWriter<W>,
) -> Result<(), RpcError>
where
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use crate::protocol::RpcMethod::*;
    match &request.method {
        SessionAttach { session_id } => {
            handle_session_attach(state, writer, &request.id, session_id.clone()).await
        }
        SessionDetach { session_id } => {
            handle_session_detach(state, writer, &request.id, session_id).await
        }
        RunStart { session_id, provider, model, prompt } => {
            handle_run_start(
                state,
                writer,
                &request.id,
                session_id.clone(),
                prompt,
                provider.clone(),
                model.clone(),
            )
            .await
        }
        RunCancel { session_id, run_id } => {
            handle_run_cancel(state, writer, &request.id, session_id, run_id.as_ref()).await
        }
        AuthStatus { provider } => handle_auth_status(state, writer, &request.id, provider).await,
        Capabilities { provider } => {
            handle_capabilities(state, writer, &request.id, provider).await
        }
    }
}

// ---------------------------------------------------------------------------
// session_attach (session_join)
// ---------------------------------------------------------------------------

pub async fn handle_session_attach<W>(
    state: &Arc<ServerState>,
    writer: &LineWriter<W>,
    request_id: &str,
    session_id: Option<SessionId>,
) -> Result<(), RpcError>
where
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let info = if let Some(sid) = session_id {
        // Look up existing session.
        let opt_info = {
            let sessions = state.sessions.lock().unwrap();
            sessions.get(&sid).map(session_to_info)
        };
        match opt_info {
            None => {
                return writer
                    .write(&RpcResponse::Error {
                        request_id: request_id.to_string(),
                        code: RpcErrorCode::SessionNotFound.to_string(),
                        message: format!("session not found: {}", sid),
                    })
                    .await;
            }
            Some(info) => info,
        }
    } else {
        // Create a new session.
        let (session, creation_event) = Session::new();
        let info = session_to_info(&session);
        {
            let mut sessions = state.sessions.lock().unwrap();
            sessions.insert(session.id.clone(), session);
        }
        state.event_bus.emit(creation_event);
        info
    };

    // Subscribe to bus and spawn a task that forwards ALL events to the writer.
    let mut rx = state.event_bus.subscribe();
    let writer_clone = writer.clone();
    let state_clone = Arc::clone(state);

    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let seq = state_clone.next_seq.fetch_add(1, Ordering::SeqCst);
                    let rpc_event = normalize_event(&event, seq);
                    let resp = RpcResponse::StreamEvent { event: rpc_event };
                    if writer_clone.write(&resp).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    });

    writer
        .write(&RpcResponse::Success {
            request_id: request_id.to_string(),
            data: serde_json::to_value(info)?,
        })
        .await
}

// ---------------------------------------------------------------------------
// session_leave (session_detach)
// ---------------------------------------------------------------------------

pub async fn handle_session_detach<W>(
    state: &Arc<ServerState>,
    writer: &LineWriter<W>,
    request_id: &str,
    session_id: &SessionId,
) -> Result<(), RpcError>
where
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let end_event = {
        let mut sessions = state.sessions.lock().unwrap();
        match sessions.get_mut(session_id) {
            None => None,
            Some(session) => Some(session.end()),
        }
    };

    match end_event {
        None => {
            writer
                .write(&RpcResponse::Error {
                    request_id: request_id.to_string(),
                    code: RpcErrorCode::SessionNotFound.to_string(),
                    message: format!("session not found: {}", session_id),
                })
                .await
        }
        Some(ev) => {
            state.event_bus.emit(ev);
            writer
                .write(&RpcResponse::Success {
                    request_id: request_id.to_string(),
                    data: json!({ "detached": true }),
                })
                .await
        }
    }
}

// ---------------------------------------------------------------------------
// run_start
// ---------------------------------------------------------------------------

pub async fn handle_run_start<W>(
    state: &Arc<ServerState>,
    writer: &LineWriter<W>,
    request_id: &str,
    session_id: Option<SessionId>,
    prompt: &str,
    provider: Option<ProviderId>,
    model: Option<ModelId>,
) -> Result<(), RpcError>
where
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    // Get or create session.
    let sid = if let Some(sid) = session_id {
        let exists = {
            let sessions = state.sessions.lock().unwrap();
            sessions.contains_key(&sid)
        };
        if !exists {
            return writer
                .write(&RpcResponse::Error {
                    request_id: request_id.to_string(),
                    code: RpcErrorCode::SessionNotFound.to_string(),
                    message: format!("session not found: {}", sid),
                })
                .await;
        }
        sid
    } else {
        let (session, ev) = Session::new();
        let sid = session.id.clone();
        {
            let mut sessions = state.sessions.lock().unwrap();
            sessions.insert(sid.clone(), session);
        }
        state.event_bus.emit(ev);
        sid
    };

    let provider = provider.unwrap_or_else(|| ProviderId::new("default"));
    let model = model.unwrap_or_else(|| ModelId::new("default"));
    let _prompt = prompt.to_string();

    // Create and transition run through Created → Queued → Running.
    let (mut run, created_ev) = Run::new(sid.clone());
    let queued_ev = run.queue().map_err(|e| RpcError::InvalidRunState(e.to_string()))?;
    let started_ev = run
        .start(RunParams {
            session_id: sid.clone(),
            provider: provider.clone(),
            model: model.clone(),
        })
        .map_err(|e| RpcError::InvalidRunState(e.to_string()))?;

    let run_id = run.id.clone();
    let cancel_token = run.cancel_token.clone();

    {
        let mut runs = state.runs.lock().unwrap();
        runs.insert(run_id.clone(), run);
    }
    {
        let mut tokens = state.cancel_tokens.lock().unwrap();
        tokens.insert(run_id.clone(), cancel_token.clone());
    }
    // Attach run to session.
    {
        let mut sessions = state.sessions.lock().unwrap();
        if let Some(session) = sessions.get_mut(&sid) {
            let _ = session.attach_run(run_id.clone());
        }
    }

    state.event_bus.emit(created_ev);
    state.event_bus.emit(queued_ev);
    state.event_bus.emit(started_ev);

    // Acknowledge immediately.
    writer.write(&RpcResponse::Ack { request_id: request_id.to_string() }).await?;

    // Spawn simulation task: 3 token chunks then RunCompleted.
    let state_clone = Arc::clone(state);
    let writer_clone = writer.clone();
    let run_id_clone = run_id.clone();

    tokio::spawn(async move {
        for i in 0..3usize {
            if cancel_token.is_cancelled() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;

            let event = AgentEvent::TokenChunk {
                run_id: run_id_clone.clone(),
                delta: format!("token chunk {}", i),
                timestamp: Utc::now(),
            };
            state_clone.event_bus.emit(event.clone());

            let seq = state_clone.next_seq.fetch_add(1, Ordering::SeqCst);
            let rpc_event = normalize_event(&event, seq);
            if writer_clone.write(&RpcResponse::StreamEvent { event: rpc_event }).await.is_err() {
                return;
            }
        }

        if !cancel_token.is_cancelled() {
            // Update run status to Completed.
            {
                let mut runs = state_clone.runs.lock().unwrap();
                if let Some(run) = runs.get_mut(&run_id_clone) {
                    let _ = run.complete();
                }
            }
            let event = AgentEvent::RunCompleted {
                run_id: run_id_clone.clone(),
                timestamp: Utc::now(),
            };
            state_clone.event_bus.emit(event.clone());

            let seq = state_clone.next_seq.fetch_add(1, Ordering::SeqCst);
            let rpc_event = normalize_event(&event, seq);
            let _ = writer_clone
                .write(&RpcResponse::StreamEvent { event: rpc_event })
                .await;
        }
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// run_cancel
// ---------------------------------------------------------------------------

pub async fn handle_run_cancel<W>(
    state: &Arc<ServerState>,
    writer: &LineWriter<W>,
    request_id: &str,
    session_id: &SessionId,
    run_id: Option<&RunId>,
) -> Result<(), RpcError>
where
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    // Resolve the target run_id.
    let target_run_id = if let Some(rid) = run_id {
        rid.clone()
    } else {
        // Use most recent run for session.
        let opt = {
            let sessions = state.sessions.lock().unwrap();
            sessions.get(session_id).and_then(|s| s.current_run_id().cloned())
        };
        match opt {
            Some(rid) => rid,
            None => {
                // Check if session exists at all.
                let exists = {
                    let sessions = state.sessions.lock().unwrap();
                    sessions.contains_key(session_id)
                };
                if !exists {
                    return writer
                        .write(&RpcResponse::Error {
                            request_id: request_id.to_string(),
                            code: RpcErrorCode::SessionNotFound.to_string(),
                            message: format!("session not found: {}", session_id),
                        })
                        .await;
                }
                return writer
                    .write(&RpcResponse::Error {
                        request_id: request_id.to_string(),
                        code: RpcErrorCode::RunNotFound.to_string(),
                        message: "no active run for session".to_string(),
                    })
                    .await;
            }
        }
    };

    // Trigger the cancellation token.
    let token = {
        let tokens = state.cancel_tokens.lock().unwrap();
        tokens.get(&target_run_id).cloned()
    };
    if let Some(t) = token {
        t.cancel();
    }

    // Transition run to Cancelled and emit events.
    let cancel_events = {
        let mut runs = state.runs.lock().unwrap();
        runs.get_mut(&target_run_id).and_then(|run| {
            if run.status.can_cancel() {
                run.cancel().ok()
            } else {
                None
            }
        })
    };
    if let Some((req_ev, cancel_ev)) = cancel_events {
        state.event_bus.emit(req_ev);
        state.event_bus.emit(cancel_ev);
    }

    writer
        .write(&RpcResponse::Success {
            request_id: request_id.to_string(),
            data: json!({ "cancelled": true, "run_id": target_run_id.to_string() }),
        })
        .await
}

// ---------------------------------------------------------------------------
// auth_status
// ---------------------------------------------------------------------------

pub async fn handle_auth_status<W>(
    _state: &Arc<ServerState>,
    writer: &LineWriter<W>,
    request_id: &str,
    provider: &ProviderId,
) -> Result<(), RpcError>
where
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let info = AuthStatusInfo {
        provider_id: provider.to_string(),
        authenticated: false,
        token_expires_at: None,
        flow: None,
    };
    writer
        .write(&RpcResponse::Success {
            request_id: request_id.to_string(),
            data: serde_json::to_value(info)?,
        })
        .await
}

// ---------------------------------------------------------------------------
// capabilities
// ---------------------------------------------------------------------------

pub async fn handle_capabilities<W>(
    _state: &Arc<ServerState>,
    writer: &LineWriter<W>,
    request_id: &str,
    _provider: &ProviderId,
) -> Result<(), RpcError>
where
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let info = CapabilitiesInfo {
        protocol_version: "1.0".to_string(),
        supported_methods: vec![
            "run_start".to_string(),
            "run_cancel".to_string(),
            "session_attach".to_string(),
            "session_detach".to_string(),
            "auth_status".to_string(),
            "capabilities".to_string(),
        ],
        streaming_supported: true,
        tool_passthrough: false,
        max_concurrent_runs: 4,
    };
    writer
        .write(&RpcResponse::Success {
            request_id: request_id.to_string(),
            data: serde_json::to_value(info)?,
        })
        .await
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn session_to_info(session: &agent_core::session::Session) -> SessionInfo {
    let status = match session.status {
        SessionStatus::Active => "active",
        SessionStatus::Idle => "idle",
        SessionStatus::Ended => "ended",
    };
    SessionInfo {
        session_id: session.id.to_string(),
        status: status.to_string(),
        created_at: session.created_at.to_rfc3339(),
        run_count: session.run_ids.len(),
        label: session.label.clone(),
    }
}

pub fn run_to_info(run: &agent_core::run::Run) -> RunInfo {
    use agent_core::run::RunStatus;
    let status = match run.status {
        RunStatus::Created => "created",
        RunStatus::Queued => "queued",
        RunStatus::Running => "running",
        RunStatus::WaitingForTool => "waiting_for_tool",
        RunStatus::Completed => "completed",
        RunStatus::Cancelled => "cancelled",
        RunStatus::Failed => "failed",
    };
    RunInfo {
        run_id: run.id.to_string(),
        session_id: run.session_id.to_string(),
        status: status.to_string(),
        created_at: run.created_at.to_rfc3339(),
        completed_at: run.completed_at.map(|t| t.to_rfc3339()),
    }
}

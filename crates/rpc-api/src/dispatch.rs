//! Method handlers for each `RpcMethod` variant.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use agent_core::run::{Run, RunParams};
use agent_core::session::Session;
use agent_core::session::SessionStatus;
use agent_core::types::{AgentEvent, ModelId, ProviderId, RunId, SessionId, ToolResult};
use chrono::Utc;
use futures::StreamExt;
use model_adapters::provider::{ChatMessage, CompletionRequest, FinishReason, MessageContent, Role};
use policy_engine::ToolRequest as PolicyToolRequest;
use serde_json::json;
use session_store::store::RunStatus;
use tool_runtime::schema::ToolConfig;

use crate::error::RpcError;
use crate::normalize::normalize_event;
use crate::protocol::{
    AuthLoginInfo, AuthStatusInfo, CapabilitiesInfo, RpcErrorCode, RpcRequest, RpcResponse,
    RunInfo, SessionInfo,
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
        AuthLogin { provider, client_id } => {
            handle_auth_login(state, writer, &request.id, provider, client_id.as_deref()).await
        }
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
        if let Some(store) = &state.session_store {
            let sid_ref = &info.session_id;
            if let Err(e) = store.create_session().await {
                tracing::warn!("failed to persist session {}: {}", sid_ref, e);
            }
        }
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
    let prompt_str = prompt.to_string();

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

    if let Some(store) = &state.run_store {
        if let Err(e) = store.create_run(sid.clone()).await {
            tracing::warn!("failed to persist run: {}", e);
        } else {
            state.event_bus.emit(AgentEvent::DataSourceAccessed {
                run_id: run_id.clone(),
                source: "postgres".to_string(),
                detail: format!("runs — created run {}", run_id),
                timestamp: Utc::now(),
            });
        }
    }

    // Acknowledge immediately.
    writer.write(&RpcResponse::Ack { request_id: request_id.to_string() }).await?;

    // Spawn real model invocation task.
    let state_clone = Arc::clone(state);
    let writer_clone = writer.clone();
    let run_id_clone = run_id.clone();
    let provider_clone = provider.clone();
    let model_clone = model.clone();
    let sid_clone = sid.clone();

    tokio::spawn(async move {
        // Fast-fail if no providers are registered.
        if state_clone.provider_registry.is_empty() {
            emit_run_failed(
                &state_clone,
                &writer_clone,
                &run_id_clone,
                "No providers configured. Add to ~/.config/rustpi/config.toml:\n\n\
                 [[providers]]\n\
                 id = \"my-llm\"\n\
                 kind = \"openai_compatible\"\n\
                 base_url = \"http://localhost:11434/v1\"  # e.g. Ollama\n\
                 # api_key = \"sk-...\"  # if required"
                    .to_string(),
            )
            .await;
            return;
        }

        // 1. Build the system prompt (with optional vault personality).
        let sys_text = build_system_message(&run_id_clone, &state_clone.event_bus).await;

        // 2. Build optional context from the working directory / memory.
        let context_text = build_context_messages(&prompt_str, &sid_clone, Arc::clone(&state_clone.memory_retriever), &run_id_clone, &state_clone.event_bus).await;

        // 3. Assemble the message list.
        //    Many local-model chat templates (Qwen, LLaMA, Mistral) require the system
        //    message to be exactly one entry at position 0.  Merge context into that
        //    single system message rather than appending a second one.
        let base_instructions = "\n\n---\n\nTool usage rules:\n\
            - When you call memory_note to save something, always tell the user the exact filename you saved it to and confirm success or failure.\n\
            - Never claim to have saved, stored, or remembered something unless you have actually called and received a successful result from the memory_note tool.\n\
            - If a tool call fails, tell the user explicitly what went wrong.";
        let combined_sys = match context_text {
            Some(ctx) => format!("{}{}\n\n---\n\n{}", sys_text, base_instructions, ctx),
            None => format!("{}{}", sys_text, base_instructions),
        };
        let mut messages: Vec<ChatMessage> = vec![ChatMessage {
            role: Role::System,
            content: MessageContent::Text(combined_sys),
        }];
        messages.push(ChatMessage {
            role: Role::User,
            content: MessageContent::Text(prompt_str.clone()),
        });

        let tool_runner = Arc::clone(&state_clone.tool_runner);
        let mut conversation = messages.clone();
        const MAX_ROUNDS: usize = 10;

        'agent: for _round in 0..MAX_ROUNDS {
            if cancel_token.is_cancelled() {
                break 'agent;
            }

            let request = CompletionRequest {
                model: model_clone.clone(),
                messages: conversation.clone(),
                max_tokens: Some(2048),
                temperature: Some(0.7),
                tools: build_tool_schemas(),
            };

            // 3. Look up provider in registry; fall back to first available when
            //    the requested ID is not explicitly registered (e.g. "default").
            let provider_arc = match state_clone.provider_registry.get(&provider_clone) {
                Some(p) => Arc::clone(p),
                None => match state_clone.provider_registry.first() {
                    Some(p) => {
                        tracing::debug!(
                            requested = %provider_clone,
                            "provider not found; using first available"
                        );
                        Arc::clone(p)
                    }
                    None => {
                        emit_run_failed(
                            &state_clone,
                            &writer_clone,
                            &run_id_clone,
                            format!("provider '{}' not found in registry", provider_clone),
                        )
                        .await;
                        return;
                    }
                },
            };

            // 4. Start streaming completion.
            // If the provider rejects tool schemas (e.g. local models whose chat template
            // does not support tool calling), transparently retry without tools so the
            // conversation continues in plain-text mode.
            let stream = match provider_arc.complete_stream(request).await {
                Ok(s) => s,
                Err(model_adapters::ProviderError::Unavailable(_) | model_adapters::ProviderError::ApiError { status: 400..=599, .. }) => {
                    tracing::warn!("provider rejected request (possibly tool-schema incompatibility); retrying without tools");
                    let fallback = CompletionRequest {
                        model: model_clone.clone(),
                        messages: conversation.clone(),
                        max_tokens: Some(2048),
                        temperature: Some(0.7),
                        tools: vec![],
                    };
                    match provider_arc.complete_stream(fallback).await {
                        Ok(s) => s,
                        Err(e) => {
                            emit_run_failed(&state_clone, &writer_clone, &run_id_clone, e.to_string()).await;
                            return;
                        }
                    }
                }
                Err(e) => {
                    emit_run_failed(&state_clone, &writer_clone, &run_id_clone, e.to_string()).await;
                    return;
                }
            };

            // 5. Forward each delta; collect tool calls and assistant text.
            let mut pending_tools: Vec<agent_core::types::ToolCall> = vec![];
            let mut assistant_text = String::new();

            let mut stream = std::pin::pin!(stream);
            while let Some(result) = stream.next().await {
                if cancel_token.is_cancelled() {
                    break 'agent;
                }
                match result {
                    Ok(delta) => {
                        if let Some(text) = &delta.text {
                            if !text.is_empty() {
                                assistant_text.push_str(text);
                                let event = AgentEvent::TokenChunk {
                                    run_id: run_id_clone.clone(),
                                    delta: text.clone(),
                                    timestamp: Utc::now(),
                                };
                                state_clone.event_bus.emit(event.clone());
                                let seq = state_clone.next_seq.fetch_add(1, Ordering::SeqCst);
                                let rpc_event = normalize_event(&event, seq);
                                if writer_clone
                                    .write(&RpcResponse::StreamEvent { event: rpc_event })
                                    .await
                                    .is_err()
                                {
                                    return;
                                }
                            }
                        }
                        if let Some(tc) = delta.tool_call {
                            pending_tools.push(tc);
                        }
                        if let Some(FinishReason::ToolCalls) = &delta.finish_reason {
                            // Tool calls finish — continue to execute them below.
                        }
                    }
                    Err(e) => {
                        emit_run_failed(
                            &state_clone,
                            &writer_clone,
                            &run_id_clone,
                            e.to_string(),
                        )
                        .await;
                        return;
                    }
                }
            }

            // Add assistant turn to conversation history.
            if !assistant_text.is_empty() {
                conversation.push(ChatMessage {
                    role: Role::Assistant,
                    content: MessageContent::Text(assistant_text),
                });
            }

            // If no tool calls were requested, the model is done.
            if pending_tools.is_empty() {
                break 'agent;
            }

            // Emit ToolCallRequested events and execute each tool call.
            for tool_call in &pending_tools {
                state_clone.event_bus.emit(AgentEvent::ToolCallRequested {
                    run_id: run_id_clone.clone(),
                    call: tool_call.clone(),
                    timestamp: Utc::now(),
                });

                // Policy check before execution.
                let policy_req = PolicyToolRequest {
                    tool_name: tool_call.name.clone(),
                    args: tool_call.arguments.clone(),
                };
                let decision = state_clone.policy_engine.evaluate_tool(&policy_req);
                if !decision.is_allowed() {
                    let reason = decision.reason.clone();
                    tracing::warn!(
                        run_id = %run_id_clone,
                        tool = %tool_call.name,
                        reason = %reason,
                        "tool call blocked by policy"
                    );

                    // Emit PolicyDenied event.
                    state_clone.event_bus.emit(AgentEvent::PolicyDenied {
                        domain: "tool".to_string(),
                        subject: tool_call.name.clone(),
                        rule: decision.matched_rule.clone(),
                        reason: reason.clone(),
                        timestamp: Utc::now(),
                    });

                    // Notify the client.
                    let denial_msg =
                        format!("\n[Policy: {} blocked — {}]\n", tool_call.name, reason);
                    let event = AgentEvent::TokenChunk {
                        run_id: run_id_clone.clone(),
                        delta: denial_msg,
                        timestamp: Utc::now(),
                    };
                    state_clone.event_bus.emit(event.clone());
                    let seq = state_clone.next_seq.fetch_add(1, Ordering::SeqCst);
                    let _ = writer_clone
                        .write(&RpcResponse::StreamEvent {
                            event: normalize_event(&event, seq),
                        })
                        .await;

                    // Feed denial back as a tool result so the model can react.
                    conversation.push(ChatMessage {
                        role: Role::Tool,
                        content: MessageContent::ToolResult {
                            call_id: tool_call.id.clone(),
                            output: json!({
                                "error": "tool call blocked by policy",
                                "reason": reason,
                                "tool": tool_call.name,
                            }),
                        },
                    });

                    continue;
                }

                let result: ToolResult = if cancel_token.is_cancelled() {
                    ToolResult {
                        call_id: tool_call.id.clone(),
                        success: false,
                        output: json!({ "error": "run cancelled" }),
                    }
                } else if tool_call.name == "memory_note" {
                    execute_memory_note(&tool_call).await
                } else {
                    match tool_runner
                        .execute(
                            tool_call.clone(),
                            ToolConfig {
                                run_id: Some(run_id_clone.clone()),
                                cancel: Some(cancel_token.clone()),
                                ..Default::default()
                            },
                        )
                        .await
                    {
                        Ok(r) => r,
                        Err(e) => ToolResult {
                            call_id: tool_call.id.clone(),
                            success: false,
                            output: json!({ "error": e.to_string() }),
                        },
                    }
                };

                // Emit tool result event.
                state_clone.event_bus.emit(AgentEvent::ToolResultSubmitted {
                    run_id: run_id_clone.clone(),
                    result: result.clone(),
                    timestamp: Utc::now(),
                });

                // Notify the client with a brief status token.
                let status = if result.success { "ok" } else { "error" };
                let tool_msg = format!("\n[Tool: {} → {}]\n", tool_call.name, status);
                let event = AgentEvent::TokenChunk {
                    run_id: run_id_clone.clone(),
                    delta: tool_msg,
                    timestamp: Utc::now(),
                };
                state_clone.event_bus.emit(event.clone());
                let seq = state_clone.next_seq.fetch_add(1, Ordering::SeqCst);
                let _ = writer_clone
                    .write(&RpcResponse::StreamEvent { event: normalize_event(&event, seq) })
                    .await;

                // Feed the result back into the conversation.
                conversation.push(ChatMessage {
                    role: Role::Tool,
                    content: MessageContent::ToolResult {
                        call_id: result.call_id.clone(),
                        output: result.output.clone(),
                    },
                });
            }
            // Continue loop: call model again with tool results in context.
        }

        // 6. Mark run completed.
        if !cancel_token.is_cancelled() {
            {
                let mut runs = state_clone.runs.lock().unwrap();
                if let Some(run) = runs.get_mut(&run_id_clone) {
                    let _ = run.complete();
                }
            }
            if let Some(store) = &state_clone.run_store {
                if let Err(e) = store.update_run_status(&run_id_clone, RunStatus::Completed).await {
                    tracing::warn!("failed to update run status: {}", e);
                }
            }
            let event = AgentEvent::RunCompleted {
                run_id: run_id_clone.clone(),
                timestamp: Utc::now(),
            };
            state_clone.event_bus.emit(event.clone());
            let seq = state_clone.next_seq.fetch_add(1, Ordering::SeqCst);
            let rpc_event = normalize_event(&event, seq);
            let _ = writer_clone.write(&RpcResponse::StreamEvent { event: rpc_event }).await;
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
    state: &Arc<ServerState>,
    writer: &LineWriter<W>,
    request_id: &str,
    provider: &ProviderId,
) -> Result<(), RpcError>
where
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use auth_core::AuthState;

    let (authenticated, token_expires_at, flow) =
        match state.token_store.load(provider).await {
            Ok(Some(AuthState::Authenticated { expires_at, .. })) => {
                let expires_str = expires_at.map(|dt| dt.to_rfc3339());
                (true, expires_str, Some("api_key".to_string()))
            }
            Ok(Some(AuthState::Pending { .. })) => {
                (false, None, Some("pending".to_string()))
            }
            Ok(Some(AuthState::Expired { .. })) => {
                (false, None, Some("expired".to_string()))
            }
            Ok(Some(AuthState::Failed { .. }))
            | Ok(Some(AuthState::Unauthenticated))
            | Ok(None)
            | Err(_) => (false, None, None),
        };

    let info = AuthStatusInfo {
        provider_id: provider.to_string(),
        authenticated,
        token_expires_at,
        flow,
    };
    writer
        .write(&RpcResponse::Success {
            request_id: request_id.to_string(),
            data: serde_json::to_value(info)?,
        })
        .await
}

// ---------------------------------------------------------------------------
// auth_login
// ---------------------------------------------------------------------------

pub async fn handle_auth_login<W>(
    state: &Arc<ServerState>,
    writer: &LineWriter<W>,
    request_id: &str,
    provider: &ProviderId,
    _client_id: Option<&str>,
) -> Result<(), RpcError>
where
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use auth_core::AuthState;

    // Check if already authenticated.
    match state.token_store.load(provider).await {
        Ok(Some(AuthState::Authenticated { .. })) => {
            let info = AuthLoginInfo {
                provider_id: provider.to_string(),
                authenticated: true,
                user_code: None,
                verification_uri: None,
                message: Some("Already authenticated.".to_string()),
            };
            return writer
                .write(&RpcResponse::Success {
                    request_id: request_id.to_string(),
                    data: serde_json::to_value(info)?,
                })
                .await;
        }
        _ => {}
    }

    // Not authenticated — the device code flow is handled in the CLI layer
    // for interactive terminals.  Return an informational response.
    let info = AuthLoginInfo {
        provider_id: provider.to_string(),
        authenticated: false,
        user_code: None,
        verification_uri: None,
        message: Some(format!(
            "Provider '{}' is not authenticated. \
             Set credentials via environment variable or run `rustpi auth login {}` \
             in an interactive terminal.",
            provider, provider
        )),
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
            "auth_login".to_string(),
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

/// Emit a RunFailed event and update the in-memory run state.
async fn emit_run_failed<W>(
    state: &Arc<ServerState>,
    writer: &LineWriter<W>,
    run_id: &RunId,
    reason: String,
) where
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    {
        let mut runs = state.runs.lock().unwrap();
        if let Some(run) = runs.get_mut(run_id) {
            let _ = run.fail(&reason);
        }
    }
    let event = AgentEvent::RunFailed {
        run_id: run_id.clone(),
        reason,
        timestamp: Utc::now(),
    };
    state.event_bus.emit(event.clone());
    let seq = state.next_seq.fetch_add(1, Ordering::SeqCst);
    let rpc_event = normalize_event(&event, seq);
    let _ = writer.write(&RpcResponse::StreamEvent { event: rpc_event }).await;
}

/// Execute the virtual `memory_note` tool: write a note to the vault's memory/ folder.
async fn execute_memory_note(call: &agent_core::types::ToolCall) -> agent_core::types::ToolResult {
    let call_id = call.id.clone();
    let args = &call.arguments;

    let title = match args.get("title").and_then(|v| v.as_str()) {
        Some(t) => t.to_string(),
        None => return agent_core::types::ToolResult {
            call_id,
            success: false,
            output: json!({ "error": "missing required argument: title" }),
        },
    };
    let content = match args.get("content").and_then(|v| v.as_str()) {
        Some(c) => c.to_string(),
        None => return agent_core::types::ToolResult {
            call_id,
            success: false,
            output: json!({ "error": "missing required argument: content" }),
        },
    };
    let append = args.get("append").and_then(|v| v.as_bool()).unwrap_or(false);

    // Sanitise title
    let safe_title = title.replace(['/', '\\', '\0'], "_");
    if safe_title.contains("..") {
        return agent_core::types::ToolResult {
            call_id,
            success: false,
            output: json!({ "error": "title must not contain path traversal" }),
        };
    }

    let config = match config_core::ConfigLoader::new().load() {
        Ok(c) => c,
        Err(e) => return agent_core::types::ToolResult {
            call_id,
            success: false,
            output: json!({ "error": format!("could not load config: {e}") }),
        },
    };
    let vault_path = match config.memory.obsidian_vault_path.as_ref() {
        Some(p) => p.clone(),
        None => return agent_core::types::ToolResult {
            call_id,
            success: false,
            output: json!({ "error": "no obsidian_vault_path configured" }),
        },
    };

    let memory_dir = vault_path.join("memory");
    if !memory_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&memory_dir) {
            return agent_core::types::ToolResult {
                call_id,
                success: false,
                output: json!({ "error": format!("could not create memory dir: {e}") }),
            };
        }
    }

    // Apply Obsidian wiki links [[note title]] to the content
    let linked_content = apply_wiki_links(&content, &vault_path, &safe_title);

    let file_path = memory_dir.join(format!("{safe_title}.md"));
    let final_content = if append && file_path.exists() {
        let mut existing = std::fs::read_to_string(&file_path).unwrap_or_default();
        existing.push('\n');
        existing.push_str(&linked_content);
        existing
    } else {
        linked_content.clone()
    };

    match std::fs::write(&file_path, &final_content) {
        Ok(_) => {
            // Fire-and-forget: also index in Qdrant for keyword search
            let mc = config.memory.clone();
            let content_clone = final_content.clone();
            let title_clone = safe_title.clone();
            tokio::spawn(async move {
                if mc.qdrant_enabled {
                    if let Some(url) = mc.qdrant_url.as_deref() {
                        if let Ok(qm) = memory_sync::qdrant::QdrantMemory::new(
                            url,
                            mc.qdrant_api_key.clone(),
                            mc.qdrant_collection_name.clone(),
                            None,
                        ) {
                            if let Err(e) = qm.store_text(
                                &content_clone,
                                vec!["memory_note".to_string(), title_clone.clone()],
                                Some(format!("vault/memory/{title_clone}.md")),
                            ).await {
                                tracing::warn!("memory_note: Qdrant index failed: {e}");
                            }
                        }
                    }
                }
            });

            agent_core::types::ToolResult {
                call_id,
                success: true,
                output: json!({
                    "saved": file_path.display().to_string(),
                    "title": safe_title,
                    "bytes": final_content.len(),
                    "wiki_links_applied": linked_content != content,
                }),
            }
        }
        Err(e) => agent_core::types::ToolResult {
            call_id,
            success: false,
            output: json!({ "error": format!("write failed: {e}") }),
        },
    }
}

/// Apply Obsidian wiki links `[[note title]]` to content.
///
/// Scans all `.md` files recursively under `vault_path`, collects their stems,
/// and replaces the first standalone occurrence of each stem in non-code-fence
/// lines with `[[stem]]`. Skips the note being written itself.
fn apply_wiki_links(content: &str, vault_path: &std::path::Path, self_title: &str) -> String {
    // Collect all vault note stems recursively
    let stems = collect_vault_stems(vault_path);
    if stems.is_empty() {
        return content.to_string();
    }

    let mut output = String::with_capacity(content.len() + 64);
    let mut in_code_fence = false;
    // Track which stems have been linked already (link once per note)
    let mut linked: std::collections::HashSet<String> = std::collections::HashSet::new();

    for line in content.lines() {
        let trimmed = line.trim_start();
        // Toggle code fence tracking
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_fence = !in_code_fence;
            output.push_str(line);
            output.push('\n');
            continue;
        }
        if in_code_fence {
            output.push_str(line);
            output.push('\n');
            continue;
        }

        let processed = apply_wiki_links_to_line(line, &stems, self_title, &mut linked);
        output.push_str(&processed);
        output.push('\n');
    }

    // Remove trailing newline added by the loop if the original had none
    if !content.ends_with('\n') && output.ends_with('\n') {
        output.pop();
    }
    output
}

/// Replace first occurrence of each vault stem in a single line with `[[stem]]`.
fn apply_wiki_links_to_line(
    line: &str,
    stems: &[(String, String)], // (lower_stem, display_stem)
    self_title: &str,
    linked: &mut std::collections::HashSet<String>,
) -> String {
    let mut result = line.to_string();

    for (lower, display) in stems {
        // Skip self-reference
        if lower == &self_title.to_lowercase() { continue; }
        // Skip already linked
        if linked.contains(lower) { continue; }
        // Skip if already a wiki link in the line
        if result.contains(&format!("[[{display}]]")) || result.contains(&format!("[[{lower}]]")) {
            linked.insert(lower.clone());
            continue;
        }

        // Find a whole-word, case-insensitive match not inside [[...]]
        if let Some(new_line) = replace_first_word_match(&result, lower, display) {
            result = new_line;
            linked.insert(lower.clone());
        }
    }
    result
}

/// Replace the first whole-word case-insensitive occurrence of `term` in `text`
/// with `[[display]]`, but only if the match is not already inside `[[...]]`.
fn replace_first_word_match(text: &str, term: &str, display: &str) -> Option<String> {
    let lower_text = text.to_lowercase();
    let lower_term = term.to_lowercase();

    let mut search_start = 0;
    while let Some(pos) = lower_text[search_start..].find(&lower_term) {
        let abs_pos = search_start + pos;
        let end_pos = abs_pos + term.len();

        // Check word boundaries
        let before_ok = abs_pos == 0 || {
            let prev = lower_text.as_bytes()[abs_pos - 1] as char;
            !prev.is_alphanumeric() && prev != '_' && prev != '-'
        };
        let after_ok = end_pos >= lower_text.len() || {
            let next = lower_text.as_bytes()[end_pos] as char;
            !next.is_alphanumeric() && next != '_' && next != '-'
        };

        if before_ok && after_ok {
            // Make sure we're not inside [[...]]
            let before = &text[..abs_pos];
            let open_brackets = before.matches("[[").count();
            let close_brackets = before.matches("]]").count();
            if open_brackets > close_brackets {
                // Inside an existing link — skip
                search_start = end_pos;
                continue;
            }
            // Also skip inline code spans `...`
            let backtick_count = before.chars().filter(|&c| c == '`').count();
            if backtick_count % 2 != 0 {
                search_start = end_pos;
                continue;
            }

            let mut out = text[..abs_pos].to_string();
            out.push_str(&format!("[[{display}]]"));
            out.push_str(&text[end_pos..]);
            return Some(out);
        }
        search_start = end_pos;
    }
    None
}

/// Recursively collect all `.md` file stems (filename without extension) from the vault.
/// Returns pairs of (lowercase_stem, display_stem).
fn collect_vault_stems(vault_path: &std::path::Path) -> Vec<(String, String)> {
    let mut stems = Vec::new();
    collect_stems_recursive(vault_path, vault_path, &mut stems);
    stems
}

fn collect_stems_recursive(
    root: &std::path::Path,
    dir: &std::path::Path,
    stems: &mut Vec<(String, String)>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip hidden dirs
            if path.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with('.')).unwrap_or(false) {
                continue;
            }
            collect_stems_recursive(root, &path, stems);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                stems.push((stem.to_lowercase(), stem.to_string()));
            }
        }
    }
}

/// Build the primary system message from the Obsidian vault personality.
/// Returns an empty string if no vault is configured or loading fails.
async fn build_system_message(run_id: &RunId, event_bus: &agent_core::bus::EventBus) -> String {
    try_load_personality(run_id, event_bus).await.unwrap_or_default()
}

/// Try to load personality sections from the configured Obsidian vault.
/// Returns `None` if no vault is configured or loading fails — always non-fatal.
async fn try_load_personality(run_id: &RunId, event_bus: &agent_core::bus::EventBus) -> Option<String> {
    let config = config_core::ConfigLoader::new().load().ok()?;
    let vault_path = config.memory.obsidian_vault_path.as_ref()?.clone();

    // Resolve personality subfolder (default: "personality")
    let subfolder = config.memory.obsidian_personality_subfolder
        .as_deref()
        .unwrap_or("personality");
    let personality_path = if subfolder.is_empty() || subfolder == "." {
        vault_path.clone()
    } else {
        vault_path.join(subfolder)
    };
    let vault_path_display = personality_path.display().to_string();

    let result = tokio::task::spawn_blocking(move || {
        let accessor = memory_sync::VaultAccessor::open(&personality_path).ok()?;
        let personality_cfg = memory_sync::PersonalityConfig::default();
        memory_sync::load_personality(&accessor, &personality_cfg).ok()
    })
    .await
    .ok()
    .flatten();

    result.map(|ctx| {
        let section_names: Vec<String> = ctx.sections
            .iter()
            .map(|s| format!("{:?}", s.source_doc))
            .collect();
        let detail = if section_names.is_empty() {
            format!("vault: {}", vault_path_display)
        } else {
            format!("vault: {} — sections: {}", vault_path_display, section_names.join(", "))
        };
        event_bus.emit(AgentEvent::DataSourceAccessed {
            run_id: run_id.clone(),
            source: "obsidian".to_string(),
            detail,
            timestamp: Utc::now(),
        });
        ctx.sections
            .iter()
            .map(|s| format!("### [{}]\n\n{}", s.source_doc.filename(), s.content.trim()))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    })
}

/// Optionally build a system context string from the working directory.
/// Returns `None` if the engine fails or finds nothing relevant — callers should continue without context.
async fn build_context_messages(
    prompt: &str,
    _session_id: &SessionId,
    memory: Arc<dyn context_engine::memory::MemoryRetriever>,
    run_id: &RunId,
    event_bus: &agent_core::bus::EventBus,
) -> Option<String> {
    use context_engine::{ContextEngine, EngineConfig, RelevanceHints};

    let keywords = extract_keywords(prompt);
    let cwd = std::env::current_dir().ok()?;
    let engine_config = EngineConfig::new(&cwd);
    let engine = ContextEngine::new(engine_config).with_memory(Arc::clone(&memory));

    let hints = RelevanceHints {
        keywords: keywords.clone(),
        referenced_paths: vec![],
        root: Some(cwd),
    };

    let engine_result = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        engine.build_context(hints, None),
    )
    .await
    .ok()
    .and_then(|r| r.ok());

    if let Some((packed, stats)) = engine_result {
        // Emit file context event
        let file_list: Vec<String> = packed.blocks.iter().map(|b| b.path.display().to_string()).collect();
        if !file_list.is_empty() {
            event_bus.emit(AgentEvent::DataSourceAccessed {
                run_id: run_id.clone(),
                source: "context_files".to_string(),
                detail: file_list.join(", "),
                timestamp: Utc::now(),
            });
        }

        // Emit separate DataSourceAccessed for vault and qdrant based on snippet provenance
        emit_memory_events(&packed.memory_snippets, run_id, event_bus, prompt);

        let has_content = !packed.blocks.is_empty() || !packed.memory_snippets.is_empty();
        if !has_content {
            // Try direct memory fallback
            return direct_memory_context(memory, &keywords, run_id, event_bus, prompt).await;
        }

        tracing::debug!(
            "context engine: {} files, {} memory snippets, ~{} tokens",
            stats.files_selected, stats.memory_snippets, packed.total_tokens
        );

        let mut ctx = String::new();
        if !packed.blocks.is_empty() {
            ctx.push_str(&format!("# Project Context ({} files)\n\n", stats.files_selected));
            for block in &packed.blocks {
                ctx.push_str(&format!("## {}\n```\n{}\n```\n\n", block.path.display(), block.content));
            }
        }
        // Include memory snippets explicitly (packed.render() would double-include file blocks)
        if !packed.memory_snippets.is_empty() {
            ctx.push_str("# Remembered Context\n\n");
            for snip in &packed.memory_snippets {
                ctx.push_str(&format!("<!-- {} -->\n{}\n\n", snip.source, snip.content));
            }
        }
        return Some(ctx);
    }

    // Context engine failed (e.g. not in a project dir) — fall back to memory-only
    direct_memory_context(memory, &keywords, run_id, event_bus, prompt).await
}

/// Retrieve memory snippets directly (bypassing file scanning) and format them as context.
async fn direct_memory_context(
    memory: Arc<dyn context_engine::memory::MemoryRetriever>,
    keywords: &[String],
    run_id: &RunId,
    event_bus: &agent_core::bus::EventBus,
    prompt: &str,
) -> Option<String> {
    use context_engine::memory::MemoryQuery;

    let query = MemoryQuery {
        keywords: keywords.to_vec(),
        max_snippets: 8,
        max_tokens_per_snippet: 600,
        total_token_budget: 3_000,
        ..Default::default()
    };
    let snippets = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        memory.retrieve(&query),
    )
    .await
    .unwrap_or_default();

    if snippets.is_empty() {
        return None;
    }

    emit_memory_events(&snippets, run_id, event_bus, prompt);

    let mut ctx = String::from("# Remembered Context\n\n");
    for snip in &snippets {
        ctx.push_str(&format!("<!-- {} -->\n{}\n\n", snip.source, snip.content));
    }
    Some(ctx)
}

/// Emit DataSourceAccessed events for vault vs qdrant memory snippets.
fn emit_memory_events(
    snippets: &[context_engine::packer::MemorySnippet],
    run_id: &RunId,
    event_bus: &agent_core::bus::EventBus,
    prompt: &str,
) {
    let vault_files: Vec<&str> = snippets.iter()
        .filter(|s| s.source.starts_with("vault/"))
        .map(|s| s.source.as_str())
        .collect();
    let qdrant_count = snippets.iter().filter(|s| s.source == "qdrant").count();

    if !vault_files.is_empty() {
        event_bus.emit(AgentEvent::DataSourceAccessed {
            run_id: run_id.clone(),
            source: "obsidian".to_string(),
            detail: vault_files.iter().map(|s| s.trim_start_matches("vault/")).collect::<Vec<_>>().join(", "),
            timestamp: Utc::now(),
        });
    }
    if qdrant_count > 0 {
        event_bus.emit(AgentEvent::DataSourceAccessed {
            run_id: run_id.clone(),
            source: "qdrant".to_string(),
            detail: format!("{} snippet(s) for: {}", qdrant_count, prompt.chars().take(80).collect::<String>()),
            timestamp: Utc::now(),
        });
    }
}

/// Extract meaningful keywords from a prompt for relevance scoring.
/// Strips punctuation, removes stop words, and filters short words.
fn extract_keywords(prompt: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "the", "a", "an", "is", "it", "in", "on", "at", "to", "for", "of", "and", "or", "but",
        "not", "with", "this", "that", "from", "by", "as", "be", "are", "was", "were", "will",
        "can", "do", "did", "has", "have", "had", "all", "any", "if", "then", "what", "how",
        "when", "where", "who", "which",
    ];
    prompt
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() >= 3)
        .filter(|w| !STOP_WORDS.contains(&w.to_lowercase().as_str()))
        .take(15)
        .map(|w| w.to_lowercase())
        .collect()
}

fn build_tool_schemas() -> Vec<serde_json::Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "shell",
                "description": "Execute a shell command. Use for running tests, builds, git commands, etc.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Shell command to run" },
                        "timeout_secs": { "type": "integer", "description": "Optional timeout override in seconds" }
                    },
                    "required": ["command"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read the contents of a file",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path to read" }
                    },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Write content to a file (creates or overwrites)",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path" },
                        "content": { "type": "string", "description": "Content to write" },
                        "create_dirs": { "type": "boolean", "description": "Create parent directories if needed" }
                    },
                    "required": ["path", "content"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "edit_file",
                "description": "Edit a file by replacing specific text",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path" },
                        "old_str": { "type": "string", "description": "Exact text to replace" },
                        "new_str": { "type": "string", "description": "Replacement text" }
                    },
                    "required": ["path", "old_str", "new_str"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "memory_note",
                "description": "Save a note to the user's memory vault (Obsidian). Use this to remember information the user asks you to store, such as credentials, preferences, facts, or any other persistent data. Notes are saved to the memory/ folder of the vault.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Filename for the note (without .md extension). Use a descriptive name like 'ssh-key-workstation' or 'api-keys'." },
                        "content": { "type": "string", "description": "Markdown content of the note to save." },
                        "append": { "type": "boolean", "description": "If true and the note already exists, append content instead of replacing it. Default: false." }
                    },
                    "required": ["title", "content"]
                }
            }
        }),
    ]
}

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

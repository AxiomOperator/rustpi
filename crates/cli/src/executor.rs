//! Executor: wraps `ServerState` and provides typed async methods for each RPC operation.
//!
//! Calls `dispatch::dispatch()` via a `tokio::io::duplex()` channel so that the dispatch
//! code is used unmodified.  The executor drops its `LineWriter` copy after dispatch
//! returns; any background tasks (e.g. the run simulation) hold their own clones and
//! the reader sees EOF only when all clones are dropped.

use std::sync::Arc;

use agent_core::types::{ModelId, ProviderId, SessionId};
use auth_core::MemoryTokenStore;
use config_core::model::Config;
use rpc_api::{
    dispatch,
    protocol::{
        AuthLoginInfo, AuthStatusInfo, CapabilitiesInfo, RpcMethod, RpcRequest, RpcResponse,
        SessionInfo,
    },
    server::ServerState,
    LineReader, LineWriter,
};
use uuid::Uuid;

use crate::error::{CliError, CliResult};

pub struct Executor {
    pub state: Arc<ServerState>,
    /// Direct reference to the in-process token store for CLI-layer writes
    /// (e.g. after completing a device code flow).
    pub token_store: Arc<MemoryTokenStore>,
}

impl Executor {
    pub fn new() -> Self {
        let store = Arc::new(MemoryTokenStore::new());
        Self {
            state: ServerState::with_registry_and_store(
                model_adapters::ProviderRegistry::new(),
                store.clone(),
            ),
            token_store: store,
        }
    }

    pub fn new_with_config(config: &Config) -> Self {
        let store = Arc::new(rpc_api::provider_factory::build_token_store_for_config(config));
        let registry = rpc_api::provider_factory::build_provider_registry(config);
        Self {
            state: ServerState::with_registry_and_store(registry, store.clone()),
            token_store: store,
        }
    }

    pub async fn new_with_config_and_persistence(config: &Config) -> Self {
        use event_log::FileEventStore;
        use session_store::factory::{build_run_store, build_session_store};

        let store = Arc::new(rpc_api::provider_factory::build_token_store_for_config(config));
        let registry = rpc_api::provider_factory::build_provider_registry(config);

        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let rustpi_dir = format!("{}/.rustpi", home);
        let event_log_path = format!("{}/events.jsonl", rustpi_dir);
        let _ = tokio::fs::create_dir_all(&rustpi_dir).await;

        // Build Qdrant memory retriever if enabled in config.
        let qdrant_retriever: Arc<dyn context_engine::memory::MemoryRetriever> = {
            let mc = &config.memory;
            if mc.qdrant_enabled {
                match mc.qdrant_url.as_deref() {
                    Some(url) => {
                        match memory_sync::qdrant::QdrantMemory::new(
                            url,
                            mc.qdrant_api_key.clone(),
                            mc.qdrant_collection_name.clone(),
                            None,
                        ) {
                            Ok(qm) => {
                                tracing::info!("Qdrant memory enabled ({})", url);
                                Arc::new(qm)
                            }
                            Err(e) => {
                                tracing::warn!("Qdrant init failed, using no-op memory: {}", e);
                                Arc::new(context_engine::memory::NoopMemory)
                            }
                        }
                    }
                    None => {
                        tracing::warn!("qdrant_enabled = true but qdrant_url is not set; using no-op memory");
                        Arc::new(context_engine::memory::NoopMemory)
                    }
                }
            } else {
                Arc::new(context_engine::memory::NoopMemory)
            }
        };

        // Build vault memory retriever from the memory/ subfolder if vault is configured.
        let vault_retriever: Arc<dyn context_engine::memory::MemoryRetriever> = {
            let mc = &config.memory;
            if let Some(vault_path) = mc.obsidian_vault_path.as_ref() {
                let memory_dir = vault_path.join("memory");
                if memory_dir.is_dir() {
                    tracing::info!("Obsidian vault memory enabled ({})", memory_dir.display());
                    Arc::new(context_engine::memory::VaultMemory::new(&memory_dir))
                } else {
                    Arc::new(context_engine::memory::NoopMemory)
                }
            } else {
                Arc::new(context_engine::memory::NoopMemory)
            }
        };

        let memory_retriever: Arc<dyn context_engine::memory::MemoryRetriever> =
            Arc::new(context_engine::memory::CombinedMemory::new(vec![qdrant_retriever, vault_retriever]));

        match (
            build_session_store(&config.memory).await,
            build_run_store(&config.memory).await,
            FileEventStore::open(&event_log_path).await,
        ) {
            (Ok(ss), Ok(rs), Ok(es)) => {
                tracing::info!("persistence initialized (SQLite + JSONL event log)");
                let state = ServerState::new_with_all_and_memory(
                    rpc_api::provider_factory::build_provider_registry(config),
                    store.clone(),
                    ss,
                    rs,
                    Arc::new(es),
                    memory_retriever,
                );
                // Reload existing sessions from store.
                if let Some(session_store) = &state.session_store {
                    match session_store.list_sessions().await {
                        Ok(records) => {
                            for record in records {
                                tracing::debug!("found existing session: {}", record.id);
                            }
                        }
                        Err(e) => tracing::warn!("failed to reload sessions: {}", e),
                    }
                }
                Self { state, token_store: store }
            }
            (ss_res, rs_res, es_res) => {
                if let Err(e) = ss_res { tracing::warn!("session store init failed: {}", e); }
                if let Err(e) = rs_res { tracing::warn!("run store init failed: {}", e); }
                if let Err(e) = es_res { tracing::warn!("event log init failed: {}", e); }
                tracing::warn!("falling back to in-memory-only mode");
                let state = ServerState::with_registry_and_store(registry, store.clone());
                Self { state, token_store: store }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Dispatch a request and collect all responses until the writer side closes.
    ///
    /// Only use this for methods that do **not** spawn indefinitely-running background
    /// tasks (AuthStatus, Capabilities, SessionDetach, RunCancel).
    async fn dispatch_sync(&self, request: RpcRequest) -> CliResult<Vec<RpcResponse>> {
        let (writer_half, reader_half) = tokio::io::duplex(65_536);
        let line_writer = LineWriter::new(writer_half);

        dispatch::dispatch(&request, &self.state, &line_writer)
            .await
            .map_err(CliError::Rpc)?;

        // Dropping our copy means the reader sees EOF once any background tasks also
        // drop their clones.
        drop(line_writer);

        let mut reader = LineReader::new(reader_half);
        let mut responses = Vec::new();
        while let Some(result) = reader.next::<RpcResponse>().await {
            responses.push(result.map_err(CliError::Rpc)?);
        }
        Ok(responses)
    }

    // -----------------------------------------------------------------------
    // Session operations
    // -----------------------------------------------------------------------

    /// List all sessions currently held in memory.
    pub fn session_list(&self) -> CliResult<Vec<SessionInfo>> {
        let sessions = self.state.sessions.lock().unwrap();
        let infos = sessions.values().map(dispatch::session_to_info).collect();
        Ok(infos)
    }

    /// Attach to an existing session or create a new one.
    ///
    /// `SessionAttach` spawns an indefinite event-forwarding task; we read the first
    /// meaningful response then drop the reader (causing that task to exit on its next
    /// write attempt).
    pub async fn session_attach(&self, session_id: Option<SessionId>) -> CliResult<SessionInfo> {
        let request = RpcRequest {
            id: Uuid::new_v4().to_string(),
            method: RpcMethod::SessionAttach { session_id },
        };

        let (writer_half, reader_half) = tokio::io::duplex(65_536);
        let line_writer = LineWriter::new(writer_half);

        dispatch::dispatch(&request, &self.state, &line_writer)
            .await
            .map_err(CliError::Rpc)?;

        // Drop writer — event-forwarder still has its clone but will get BrokenPipe
        // when we drop the reader after reading the response.
        drop(line_writer);

        let mut reader = LineReader::new(reader_half);
        while let Some(result) = reader.next::<RpcResponse>().await {
            match result.map_err(CliError::Rpc)? {
                RpcResponse::Success { data, .. } => {
                    return serde_json::from_value::<SessionInfo>(data)
                        .map_err(|e| CliError::Other(e.to_string()));
                }
                RpcResponse::Error { code, message, .. } => {
                    return Err(map_rpc_error(&code, message));
                }
                _ => continue,
            }
        }

        Err(CliError::Other("no response from server for session_attach".into()))
    }

    /// Detach from (end) a session.
    pub async fn session_detach(&self, session_id: SessionId) -> CliResult<()> {
        let request = RpcRequest {
            id: Uuid::new_v4().to_string(),
            method: RpcMethod::SessionDetach { session_id },
        };

        for resp in self.dispatch_sync(request).await? {
            match resp {
                RpcResponse::Success { .. } => return Ok(()),
                RpcResponse::Error { code, message, .. } => {
                    return Err(map_rpc_error(&code, message));
                }
                _ => continue,
            }
        }
        Err(CliError::Other("no response from server for session_detach".into()))
    }

    /// Return info for a specific session, or `SessionNotFound` if absent.
    pub fn session_info(&self, session_id: &SessionId) -> CliResult<SessionInfo> {
        let sessions = self.state.sessions.lock().unwrap();
        sessions
            .get(session_id)
            .map(dispatch::session_to_info)
            .ok_or_else(|| CliError::SessionNotFound(session_id.to_string()))
    }

    // -----------------------------------------------------------------------
    // Run operations
    // -----------------------------------------------------------------------

    /// Start a run, delivering each `RpcResponse` to the callback as it arrives.
    ///
    /// The callback returns `true` to continue reading or `false` to stop early.
    /// Stops automatically on `run_completed` / `run_failed` / `run_cancelled` events
    /// (the callback is responsible for detecting these).
    pub async fn run_start<F>(
        &self,
        session_id: Option<SessionId>,
        provider: Option<ProviderId>,
        model: Option<ModelId>,
        prompt: String,
        mut on_response: F,
    ) -> CliResult<()>
    where
        F: FnMut(RpcResponse) -> bool,
    {
        let request = RpcRequest {
            id: Uuid::new_v4().to_string(),
            method: RpcMethod::RunStart { session_id, provider, model, prompt },
        };

        // Use a generous buffer; the run simulation emits a handful of small messages.
        let (writer_half, reader_half) = tokio::io::duplex(1_048_576);
        let line_writer = LineWriter::new(writer_half);

        dispatch::dispatch(&request, &self.state, &line_writer)
            .await
            .map_err(CliError::Rpc)?;

        // Drop our copy; the background simulation task holds its own clone and the
        // reader sees EOF when that task completes and drops its clone.
        drop(line_writer);

        let mut reader = LineReader::new(reader_half);
        while let Some(result) = reader.next::<RpcResponse>().await {
            let resp = result.map_err(CliError::Rpc)?;
            if !on_response(resp) {
                break;
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Auth operations
    // -----------------------------------------------------------------------

    /// Query authentication status for a provider.
    pub async fn auth_status(&self, provider: ProviderId) -> CliResult<AuthStatusInfo> {
        let request = RpcRequest {
            id: Uuid::new_v4().to_string(),
            method: RpcMethod::AuthStatus { provider },
        };
        for resp in self.dispatch_sync(request).await? {
            match resp {
                RpcResponse::Success { data, .. } => {
                    return serde_json::from_value::<AuthStatusInfo>(data)
                        .map_err(|e| CliError::Other(e.to_string()));
                }
                RpcResponse::Error { code, message, .. } => {
                    return Err(map_rpc_error(&code, message));
                }
                _ => continue,
            }
        }
        Err(CliError::Other("no response from server for auth_status".into()))
    }

    /// Initiate authentication for a provider.
    pub async fn auth_login(
        &self,
        provider: ProviderId,
        client_id: Option<String>,
    ) -> CliResult<AuthLoginInfo> {
        let request = RpcRequest {
            id: Uuid::new_v4().to_string(),
            method: RpcMethod::AuthLogin { provider, client_id },
        };
        for resp in self.dispatch_sync(request).await? {
            match resp {
                RpcResponse::Success { data, .. } => {
                    return serde_json::from_value::<AuthLoginInfo>(data)
                        .map_err(|e| CliError::Other(e.to_string()));
                }
                RpcResponse::Error { code, message, .. } => {
                    return Err(map_rpc_error(&code, message));
                }
                _ => continue,
            }
        }
        Err(CliError::Other("no response from server for auth_login".into()))
    }

    // -----------------------------------------------------------------------
    // Capability query
    // -----------------------------------------------------------------------

    /// Query capability info for a provider.
    pub async fn capabilities(&self, provider: ProviderId) -> CliResult<CapabilitiesInfo> {
        let request = RpcRequest {
            id: Uuid::new_v4().to_string(),
            method: RpcMethod::Capabilities { provider },
        };
        for resp in self.dispatch_sync(request).await? {
            match resp {
                RpcResponse::Success { data, .. } => {
                    return serde_json::from_value::<CapabilitiesInfo>(data)
                        .map_err(|e| CliError::Other(e.to_string()));
                }
                RpcResponse::Error { code, message, .. } => {
                    return Err(map_rpc_error(&code, message));
                }
                _ => continue,
            }
        }
        Err(CliError::Other("no response from server for capabilities".into()))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map an RPC error code string to the appropriate `CliError` variant.
fn map_rpc_error(code: &str, message: String) -> CliError {
    match code {
        "session_not_found" => CliError::SessionNotFound(message),
        "auth_unavailable" => CliError::AuthRequired(message),
        _ => CliError::Other(format!("[{}] {}", code, message)),
    }
}

/// Parse a session UUID string into a `SessionId`, returning `CliError::InvalidArgs` on failure.
pub fn parse_session_id(s: &str) -> CliResult<SessionId> {
    uuid::Uuid::parse_str(s)
        .map(SessionId)
        .map_err(|e| CliError::InvalidArgs(format!("invalid session UUID '{}': {}", s, e)))
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("run {0} not found")]
    RunNotFound(crate::types::RunId),
    #[error("session {0} not found")]
    SessionNotFound(crate::types::SessionId),
    #[error("invalid run state transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },
    #[error("run {0} is already cancelled")]
    AlreadyCancelled(crate::types::RunId),
    #[error("run {0} is already completed")]
    AlreadyCompleted(crate::types::RunId),
    #[error("operation cancelled")]
    Cancelled,
    #[error("context budget exceeded: {0} tokens")]
    ContextBudgetExceeded(u32),
    #[error("event bus closed")]
    BusClosed,
    #[error("prompt assembly error: {0}")]
    PromptAssembly(String),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
}

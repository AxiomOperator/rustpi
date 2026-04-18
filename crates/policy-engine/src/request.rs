//! Typed request types for each policy evaluation area.

/// A request to evaluate whether a tool may be executed.
#[derive(Debug, Clone)]
pub struct ToolRequest {
    pub tool_name: String,
    /// Tool arguments for future content-based rules.
    pub args: serde_json::Value,
}

/// A request to evaluate whether a file path may be mutated.
#[derive(Debug, Clone)]
pub struct FileMutationRequest {
    pub path: std::path::PathBuf,
    pub operation: FileOperation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOperation {
    Read,
    Write,
    Delete,
    Create,
}

/// A request to evaluate whether a provider may be used.
#[derive(Debug, Clone)]
pub struct ProviderRequest {
    pub provider_id: String,
    pub model_id: Option<String>,
    pub operation: ProviderOperation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderOperation {
    Chat,
    Embedding,
    ModelList,
}

/// A request to evaluate auth-related actions.
#[derive(Debug, Clone)]
pub struct AuthRequest {
    pub provider_id: String,
    pub action: AuthAction,
    pub is_authenticated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthAction {
    Login,
    Refresh,
    UseToken,
}

//! Configuration model types.

use agent_core::types::{ModelId, ProviderId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level runtime configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub global: GlobalConfig,
    pub providers: Vec<ProviderConfig>,
    pub memory: MemoryConfig,
    pub user: UserConfig,
    pub project: ProjectConfig,
    pub cli: CliConfig,
    pub tui: TuiConfig,
    pub logging: LoggingConfig,
    pub policy: PolicyDefaults,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GlobalConfig {
    /// Default provider to use when none is specified.
    pub default_provider: Option<ProviderId>,
    /// Default model to use when none is specified.
    pub default_model: Option<ModelId>,
    /// Maximum tokens per context window.
    pub max_context_tokens: Option<u32>,
    /// Log level (trace, debug, info, warn, error).
    pub log_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: ProviderId,
    pub kind: ProviderKind,
    /// Base URL override (e.g. for OpenAI-compatible local endpoints).
    pub base_url: Option<String>,
    /// Auth configuration for this provider.
    pub auth: ProviderAuthConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenAiCompatible,
    LlamaCpp,
    Vllm,
    GithubCopilot,
    Gemini,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderAuthConfig {
    ApiKey { env_var: String },
    OAuthBrowser,
    DeviceCode,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MemoryConfig {
    /// Path to the Obsidian vault for human-readable memory.
    pub obsidian_vault_path: Option<PathBuf>,
    /// Session storage backend.
    pub session_backend: SessionBackend,
    /// Whether to enable Qdrant vector memory.
    pub qdrant_enabled: bool,
    /// Qdrant gRPC/HTTP endpoint URL.
    pub qdrant_url: Option<String>,
    /// API key for authenticating with Qdrant (required for cloud/secured instances).
    pub qdrant_api_key: Option<String>,
    /// Qdrant collection name to store memory points in.
    /// Defaults to "rustpi_memory" when unset.
    pub qdrant_collection_name: Option<String>,
    /// PostgreSQL connection URL (used when session_backend = "postgres").
    pub postgres_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionBackend {
    #[default]
    Sqlite,
    Sled,
    Postgres,
}

/// User-level preferences, typically from `~/.rustpi/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct UserConfig {
    /// Preferred provider for interactive sessions.
    pub preferred_provider: Option<ProviderId>,
    /// Preferred model for interactive sessions.
    pub preferred_model: Option<ModelId>,
    /// TUI color theme name.
    pub theme: Option<String>,
    /// Preferred editor for multi-line input (e.g. "vim", "nano").
    pub editor: Option<String>,
}

/// Project-scoped settings, loaded from `.rustpi/config.toml` in the project directory.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectConfig {
    pub project_name: Option<String>,
    pub default_provider: Option<ProviderId>,
    pub default_model: Option<ModelId>,
    /// Allowlist of tool names permitted in this project.
    pub allowed_tools: Option<Vec<String>>,
    /// Root directory for context scanning.
    pub context_root: Option<PathBuf>,
}

/// CLI interface settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CliConfig {
    pub output_format: OutputFormat,
    /// Whether to use ANSI color codes in output.
    pub color: bool,
    /// Whether to pipe long output through a pager.
    pub pager: bool,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            output_format: OutputFormat::default(),
            color: true,
            pager: true,
        }
    }
}

/// TUI interface settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TuiConfig {
    pub theme: String,
    pub show_token_count: bool,
    pub wrap_lines: bool,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            theme: "default".to_string(),
            show_token_count: true,
            wrap_lines: true,
        }
    }
}

/// Logging / tracing settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
    pub format: LogFormat,
    /// Optional file path to write logs to.
    pub file: Option<PathBuf>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: LogFormat::default(),
            file: None,
        }
    }
}

/// Policy default behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PolicyDefaults {
    pub default_tool_policy: DefaultPolicy,
    pub require_approval_for_file_writes: bool,
    pub require_approval_for_shell_commands: bool,
}

impl Default for PolicyDefaults {
    fn default() -> Self {
        Self {
            default_tool_policy: DefaultPolicy::default(),
            require_approval_for_file_writes: false,
            require_approval_for_shell_commands: false,
        }
    }
}

/// Output format for the CLI.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Jsonl,
}

/// Log output format.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    #[default]
    Pretty,
    Json,
}

/// Default policy for tool execution.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DefaultPolicy {
    #[default]
    Allow,
    Deny,
}

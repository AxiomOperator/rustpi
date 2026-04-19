//! CLI argument definitions via clap derive.

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// rustpi — async AI agent platform CLI.
#[derive(Parser, Debug)]
#[command(
    name = "rustpi",
    about = "rustpi agent CLI — async AI agent platform",
    version,
    propagate_version = true
)]
pub struct Cli {
    /// Output format: `print` (human-readable) or `json` (machine-readable JSONL).
    #[arg(long, value_enum, default_value_t = OutputFormat::Print, global = true)]
    pub output: OutputFormat,

    /// Override the default provider.
    #[arg(long, global = true)]
    pub provider: Option<String>,

    /// Override the default model.
    #[arg(long, global = true)]
    pub model: Option<String>,

    /// Attach to an existing session by UUID.
    #[arg(long, global = true)]
    pub session_id: Option<String>,

    /// Fail instead of prompting for interactive input.
    #[arg(long, global = true)]
    pub non_interactive: bool,

    /// Override the config file path.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Output format selector.
#[derive(ValueEnum, Clone, Debug, PartialEq)]
pub enum OutputFormat {
    Print,
    Json,
}

/// Top-level subcommands.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Submit a prompt and stream the response.
    Run(RunArgs),

    /// Manage agent sessions.
    Session {
        #[command(subcommand)]
        subcommand: SessionCommand,
    },

    /// Manage provider authentication.
    Auth {
        #[command(subcommand)]
        subcommand: AuthCommand,
    },

    /// Print diagnostics: config, providers, backend info, auth status.
    Diag,
}

/// Arguments for `rustpi run`.
#[derive(Args, Debug)]
pub struct RunArgs {
    /// The prompt text to submit (reads from stdin if omitted and stdin is a pipe).
    pub prompt: Option<String>,

    /// Read the prompt from a file instead of the positional argument.
    #[arg(long)]
    pub file: Option<PathBuf>,

    /// Stream output tokens as they arrive (default: true).
    #[arg(long, default_value_t = true)]
    pub stream: bool,
}

/// Subcommands for `rustpi session`.
#[derive(Subcommand, Debug)]
pub enum SessionCommand {
    /// List all in-memory sessions.
    List,

    /// Attach to an existing session or create a new one.
    Attach {
        /// Session UUID to attach to (creates a new session if omitted).
        #[arg(long)]
        id: Option<String>,
    },

    /// Detach from (end) a session.
    Detach {
        /// Session UUID to detach from.
        id: String,
    },

    /// Show details for a specific session.
    Info {
        /// Session UUID.
        id: String,
    },
}

/// Subcommands for `rustpi auth`.
#[derive(Subcommand, Debug)]
pub enum AuthCommand {
    /// Show authentication status for a provider.
    Status {
        /// Provider ID (uses default or "default" if omitted).
        #[arg(long)]
        provider: Option<String>,
    },

    /// Initiate an authentication flow for a provider.
    Login {
        /// Provider to authenticate with.
        #[arg(long)]
        provider: String,
    },

    /// Clear authentication state for a provider.
    Logout {
        /// Provider to log out from.
        #[arg(long)]
        provider: String,
    },
}

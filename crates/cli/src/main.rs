//! `rustpi` — async AI agent platform CLI.
//!
//! # Architecture
//! The CLI creates a `ServerState` in-process and calls `dispatch::dispatch()` directly,
//! bypassing the stdin/stdout JSONL transport.  A `tokio::io::duplex()` channel is used
//! inside the `Executor` to pass responses from the dispatch layer back to the CLI.

use clap::Parser;
use config_core::loader::ConfigLoader;

use agent_core::types::{ModelId, ProviderId, SessionId};
use cli::args::{Cli, Command, OutputFormat};
use cli::executor::{parse_session_id, Executor};
use cli::output::Output;

#[tokio::main]
async fn main() {
    let exit_code = run().await;
    std::process::exit(exit_code);
}

async fn run() -> i32 {
    let parsed_args = Cli::parse();

    // Initialise logging to stderr so stdout remains clean for command output.
    init_logging(&parsed_args);

    // Load layered config; --config overrides the project-level path.
    let config = {
        let mut loader = ConfigLoader::new();
        if let Some(path) = &parsed_args.config {
            loader = loader.with_project_path(path.clone());
        }
        match loader.load() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error: failed to load config: {}", e);
                return 1;
            }
        }
    };

    // Build output helper.
    let output = Output::new(parsed_args.output.clone(), config.cli.color);

    // Build executor (creates a fresh in-process ServerState).
    let executor = Executor::new();

    // Resolve global overrides.
    let provider: Option<ProviderId> = parsed_args
        .provider
        .as_deref()
        .map(ProviderId::new)
        .or_else(|| config.global.default_provider.clone());

    let model: Option<ModelId> = parsed_args
        .model
        .as_deref()
        .map(ModelId::new)
        .or_else(|| config.global.default_model.clone());

    let session_id: Option<SessionId> = match parsed_args.session_id.as_deref().map(parse_session_id) {
        Some(Err(e)) => {
            output.print_err(&e.to_string());
            return e.exit_code();
        }
        Some(Ok(id)) => Some(id),
        None => None,
    };

    let result = match parsed_args.command {
        None => {
            // No subcommand: print help.
            use clap::CommandFactory;
            let mut cmd = Cli::command();
            cmd.print_help().unwrap();
            println!();
            return 0;
        }

        Some(Command::Run(ref args)) => {
            cli::commands::run::run_command(
                args,
                provider,
                model,
                session_id,
                parsed_args.non_interactive,
                &output,
                &executor,
            )
            .await
        }

        Some(Command::Session { subcommand }) => {
            cli::commands::session::session_command(subcommand, &output, &executor).await
        }

        Some(Command::Auth { subcommand }) => {
            cli::commands::auth::auth_command(
                subcommand,
                &config,
                &output,
                &executor,
                parsed_args.non_interactive,
            )
            .await
        }

        Some(Command::Diag) => {
            cli::commands::diag::diag_command(&config, &output, &executor).await
        }

        Some(Command::Replay { session_id, audit_only, failures_only }) => {
            cli::commands::replay::replay_command(
                session_id.as_deref(),
                audit_only,
                failures_only,
                &output,
            )
            .await
        }
    };

    match result {
        Ok(()) => 0,
        Err(e) => {
            output.print_err(&e.to_string());
            e.exit_code()
        }
    }
}

fn init_logging(parsed_args: &Cli) {
    // Write logs to stderr only; stdout must stay clean for command output.
    // Respect RUST_LOG, fall back to warn so routine commands are quiet.
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| {
        if parsed_args.output == OutputFormat::Json {
            "error".to_string()
        } else {
            "warn".to_string()
        }
    });

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .with_writer(std::io::stderr)
        .init();
}

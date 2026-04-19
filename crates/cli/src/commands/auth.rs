//! `rustpi auth` subcommands.

use agent_core::types::ProviderId;
use config_core::model::Config;

use crate::{
    args::{AuthCommand, OutputFormat},
    error::{CliError, CliResult},
    executor::Executor,
    output::Output,
};

pub async fn auth_command(
    subcommand: AuthCommand,
    config: &Config,
    output: &Output,
    executor: &Executor,
    non_interactive: bool,
) -> CliResult<()> {
    match subcommand {
        AuthCommand::Status { provider } => status(provider, config, output, executor).await,
        AuthCommand::Login { provider } => login(provider, output, executor, non_interactive).await,
        AuthCommand::Logout { provider } => logout(provider, output, executor).await,
    }
}

async fn status(
    provider: Option<String>,
    config: &Config,
    output: &Output,
    executor: &Executor,
) -> CliResult<()> {
    // Resolve provider: flag > config default > "default".
    let provider_id = provider
        .as_deref()
        .map(ProviderId::new)
        .or_else(|| config.global.default_provider.clone())
        .unwrap_or_else(|| ProviderId::new("default"));

    let info = executor.auth_status(provider_id).await?;

    if output.format == OutputFormat::Json {
        let data = serde_json::to_value(&info)
            .map_err(|e| CliError::Other(e.to_string()))?;
        output.print_success("", &data);
    } else {
        output.print_header("Auth Status");
        output.print_kv("provider", &info.provider_id);
        output.print_kv(
            "authenticated",
            if info.authenticated { "yes" } else { "no" },
        );
        output.print_kv(
            "expires_at",
            info.token_expires_at.as_deref().unwrap_or("-"),
        );
        output.print_kv("flow", info.flow.as_deref().unwrap_or("-"));
        output.print_blank();
    }
    Ok(())
}

async fn login(
    provider: String,
    output: &Output,
    executor: &Executor,
    non_interactive: bool,
) -> CliResult<()> {
    if non_interactive {
        return Err(CliError::InvalidArgs(
            "--non-interactive: auth flow requires user interaction".into(),
        ));
    }

    // Check current auth state first (real RPC call).
    let info = executor.auth_status(ProviderId::new(&provider)).await?;

    if info.authenticated {
        if output.format == OutputFormat::Json {
            output.print_success(
                "",
                &serde_json::json!({
                    "provider": provider,
                    "already_authenticated": true,
                }),
            );
        } else {
            println!("Provider '{}' is already authenticated.", provider);
        }
        return Ok(());
    }

    // The dispatch layer does not yet have a LoginStart method; report clearly.
    if output.format == OutputFormat::Json {
        output.print_success(
            "",
            &serde_json::json!({
                "provider": provider,
                "status": "login_not_implemented",
                "message": "Backend login flow not yet available; set credentials via environment variable or config.",
            }),
        );
    } else {
        println!(
            "Auth login for '{}': backend login flow is not yet implemented.",
            provider
        );
        println!(
            "Set credentials via environment variable or the [providers] section in config.toml."
        );
    }

    Err(CliError::AuthFailed(format!(
        "login flow not yet implemented for provider '{}'",
        provider
    )))
}

async fn logout(provider: String, output: &Output, executor: &Executor) -> CliResult<()> {
    // Check current status (real RPC call).
    let info = executor.auth_status(ProviderId::new(&provider)).await?;

    if !info.authenticated {
        if output.format == OutputFormat::Json {
            output.print_success(
                "",
                &serde_json::json!({
                    "provider": provider,
                    "already_unauthenticated": true,
                }),
            );
        } else {
            println!("Provider '{}' is not authenticated; nothing to clear.", provider);
        }
        return Ok(());
    }

    // Backend has no ClearAuth method yet; report clearly.
    if output.format == OutputFormat::Json {
        output.print_success(
            "",
            &serde_json::json!({
                "provider": provider,
                "status": "logout_not_implemented",
                "message": "Backend logout flow not yet available.",
            }),
        );
    } else {
        println!(
            "Auth logout for '{}': backend logout method is not yet implemented.",
            provider
        );
    }

    Err(CliError::Other(format!(
        "logout not yet implemented for provider '{}'",
        provider
    )))
}

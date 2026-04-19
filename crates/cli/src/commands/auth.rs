//! `rustpi auth` subcommands.

use agent_core::types::ProviderId;
use auth_core::{
    record::TokenRecord, AuthFlow, DeviceFlow, DeviceFlowConfig, DeviceFlowResult,
};
use chrono::Utc;
use config_core::model::{Config, ProviderAuthConfig, ProviderKind};
use tokio_util::sync::CancellationToken;

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
        AuthCommand::Login { provider } => {
            login(provider, config, output, executor, non_interactive).await
        }
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
    config: &Config,
    output: &Output,
    executor: &Executor,
    non_interactive: bool,
) -> CliResult<()> {
    if non_interactive {
        return Err(CliError::InvalidArgs(
            "--non-interactive: auth flow requires user interaction".into(),
        ));
    }

    let provider_id = ProviderId::new(&provider);

    // Check current auth state first.
    let info = executor.auth_status(provider_id.clone()).await?;
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

    // Find the provider in config to determine the auth flow.
    let provider_cfg = config.providers.iter().find(|p| p.id == provider_id);

    match provider_cfg.map(|p| (&p.kind, &p.auth)) {
        Some((_, ProviderAuthConfig::ApiKey { env_var })) => {
            // API key provider: guide the user to set the env var.
            if output.format == OutputFormat::Json {
                output.print_success(
                    "",
                    &serde_json::json!({
                        "provider": provider,
                        "auth_type": "api_key",
                        "env_var": env_var,
                        "message": format!("Set the {} environment variable to authenticate.", env_var),
                    }),
                );
            } else {
                println!("Provider '{}' uses API key authentication.", provider);
                println!("Set the following environment variable:");
                println!("  export {}=<your-api-key>", env_var);
            }
            Ok(())
        }

        Some((ProviderKind::GithubCopilot, ProviderAuthConfig::DeviceCode)) => {
            run_github_copilot_device_flow(&provider, executor, output).await
        }

        Some((_, ProviderAuthConfig::DeviceCode)) => {
            // Generic device code provider — URLs not in config yet.
            if output.format == OutputFormat::Json {
                output.print_success(
                    "",
                    &serde_json::json!({
                        "provider": provider,
                        "auth_type": "device_code",
                        "message": "Device code flow for this provider requires additional configuration (device_auth_url, token_url, client_id).",
                    }),
                );
            } else {
                println!(
                    "Provider '{}' uses device code authentication, but the required \
                     OAuth parameters (device_auth_url, token_url, client_id) are not yet \
                     configured.",
                    provider
                );
            }
            Ok(())
        }

        Some((_, ProviderAuthConfig::OAuthBrowser)) => {
            if output.format == OutputFormat::Json {
                output.print_success(
                    "",
                    &serde_json::json!({
                        "provider": provider,
                        "auth_type": "oauth_browser",
                        "message": "Browser OAuth flow is not yet implemented.",
                    }),
                );
            } else {
                println!(
                    "Provider '{}' uses browser OAuth, which is not yet implemented.",
                    provider
                );
            }
            Ok(())
        }

        None => {
            Err(CliError::InvalidArgs(format!(
                "Provider '{}' is not configured. Add it to your config.toml.",
                provider
            )))
        }
    }
}

/// Run the GitHub Copilot device code flow interactively.
async fn run_github_copilot_device_flow(
    provider: &str,
    executor: &Executor,
    output: &Output,
) -> CliResult<()> {
    // Well-known public OAuth parameters for GitHub Copilot device flow.
    // The client_id is the publicly documented GitHub Copilot neovim plugin client ID.
    let config = DeviceFlowConfig {
        provider_id: ProviderId::new(provider),
        client_id: "Iv1.b507a08c87ecfe98".to_string(),
        device_auth_url: "https://github.com/login/device/code".to_string(),
        token_url: "https://github.com/login/oauth/access_token".to_string(),
        scopes: vec!["read:user".to_string()],
    };

    let flow = DeviceFlow::new(config);

    if output.format != OutputFormat::Json {
        println!("Requesting device code from GitHub...");
    }

    let device_code = flow
        .request_device_code()
        .await
        .map_err(|e| CliError::AuthFailed(e.to_string()))?;

    if output.format == OutputFormat::Json {
        output.print_success(
            "",
            &serde_json::json!({
                "provider": provider,
                "user_code": device_code.user_code,
                "verification_uri": device_code.verification_uri,
                "message": "Visit the URL and enter the code to complete authentication.",
            }),
        );
    } else {
        println!();
        println!("  Visit:  {}", device_code.verification_uri);
        println!("  Enter:  {}", device_code.user_code);
        println!();
        println!("Waiting for authorization (press Ctrl+C to cancel)...");
    }

    let cancel_token = CancellationToken::new();
    let result = flow
        .poll_for_token(&device_code, cancel_token)
        .await
        .map_err(|e| CliError::AuthFailed(e.to_string()))?;

    match result {
        DeviceFlowResult::Success(token_response) => {
            let expires_at = token_response.expires_in.map(|secs| {
                Utc::now() + chrono::Duration::seconds(secs as i64)
            });

            let record = TokenRecord {
                provider_id: ProviderId::new(provider),
                access_token: token_response.access_token,
                refresh_token: token_response.refresh_token,
                expires_at,
                scopes: token_response
                    .scope
                    .as_deref()
                    .unwrap_or("")
                    .split_whitespace()
                    .map(str::to_string)
                    .collect(),
                flow: AuthFlow::DeviceCode,
                stored_at: Utc::now(),
            };

            executor.token_store.save_record(record);

            if output.format == OutputFormat::Json {
                output.print_success(
                    "",
                    &serde_json::json!({
                        "provider": provider,
                        "authenticated": true,
                    }),
                );
            } else {
                println!("✓ Successfully authenticated as provider '{}'.", provider);
            }

            Ok(())
        }
        DeviceFlowResult::Expired => Err(CliError::AuthFailed(
            "Device code expired before authorization was completed.".into(),
        )),
        DeviceFlowResult::Cancelled => Err(CliError::AuthFailed(
            "Authentication was cancelled.".into(),
        )),
    }
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

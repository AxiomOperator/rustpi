//! `rustpi diag` — print diagnostics: config, providers, backend info, auth status.

use agent_core::types::ProviderId;
use config_core::model::{Config, OutputFormat as CfgOutputFormat, ProviderAuthConfig, ProviderKind};
use event_log::{FileEventStore, ReplayReader};

use crate::{
    args::OutputFormat,
    error::{CliError, CliResult},
    executor::Executor,
    output::Output,
};

pub async fn diag_command(
    config: &Config,
    output: &Output,
    executor: &Executor,
) -> CliResult<()> {
    if output.format == OutputFormat::Json {
        diag_json(config, output, executor).await
    } else {
        diag_print(config, output, executor).await
    }
}

async fn diag_print(config: &Config, output: &Output, executor: &Executor) -> CliResult<()> {
    println!("rustpi diagnostics");
    println!("{}", "═".repeat(50));

    // --- Global ---
    output.print_header("Global Config");
    output.print_kv(
        "default_provider",
        &opt_str(config.global.default_provider.as_ref().map(|p| p.0.as_str())),
    );
    output.print_kv(
        "default_model",
        &opt_str(config.global.default_model.as_ref().map(|m| m.0.as_str())),
    );
    output.print_kv(
        "log_level",
        config.global.log_level.as_deref().unwrap_or(&config.logging.level),
    );
    output.print_kv(
        "max_context_tokens",
        &opt_str(config.global.max_context_tokens.map(|n| n.to_string()).as_deref()),
    );

    // --- CLI ---
    output.print_header("CLI Config");
    let fmt = match config.cli.output_format {
        CfgOutputFormat::Text => "text",
        CfgOutputFormat::Json => "json",
        CfgOutputFormat::Jsonl => "jsonl",
    };
    output.print_kv("output_format", fmt);
    output.print_kv("color", if config.cli.color { "yes" } else { "no" });
    output.print_kv("pager", if config.cli.pager { "yes" } else { "no" });

    // --- Providers ---
    output.print_header(&format!("Providers ({} configured)", config.providers.len()));
    if config.providers.is_empty() {
        println!("  (none)");
    } else {
        for p in &config.providers {
            let kind = provider_kind_str(&p.kind);
            let auth = provider_auth_str(&p.auth);
            println!("  [{}]  kind={}  auth={}", p.id, kind, auth);
            if let Some(url) = &p.base_url {
                println!("       base_url={}", url);
            }
        }
    }

    // --- Auth Status ---
    output.print_header("Auth Status");
    let probe_providers: Vec<ProviderId> = if config.providers.is_empty() {
        vec![
            config
                .global
                .default_provider
                .clone()
                .unwrap_or_else(|| ProviderId::new("default")),
        ]
    } else {
        config.providers.iter().map(|p| p.id.clone()).collect()
    };

    for provider_id in &probe_providers {
        match executor.auth_status(provider_id.clone()).await {
            Ok(info) => {
                let status = if info.authenticated {
                    format!(
                        "authenticated{}",
                        info.token_expires_at
                            .as_deref()
                            .map(|e| format!(" (expires {})", e))
                            .unwrap_or_default()
                    )
                } else {
                    "not authenticated".to_string()
                };
                output.print_kv(&provider_id.0, &status);
            }
            Err(e) => {
                output.print_kv(&provider_id.0, &format!("error: {}", e));
            }
        }
    }

    // --- Capabilities ---
    output.print_header("Backend Capabilities");
    let cap_provider = config
        .global
        .default_provider
        .clone()
        .unwrap_or_else(|| ProviderId::new("default"));
    match executor.capabilities(cap_provider.clone()).await {
        Ok(caps) => {
            output.print_kv("protocol_version", &caps.protocol_version);
            output.print_kv("supported_methods", &caps.supported_methods.join(", "));
            output.print_kv("streaming", if caps.streaming_supported { "yes" } else { "no" });
            output.print_kv(
                "tool_passthrough",
                if caps.tool_passthrough { "yes" } else { "no" },
            );
            output.print_kv("max_concurrent_runs", &caps.max_concurrent_runs.to_string());
        }
        Err(e) => {
            println!("  (query failed: {})", e);
        }
    }

    // --- Sessions ---
    let sessions = executor.session_list()?;
    output.print_header(&format!("Sessions ({} active, in-memory only)", sessions.len()));
    if !sessions.is_empty() {
        for s in &sessions {
            println!("  {} — {} ({} runs)", s.session_id, s.status, s.run_count);
        }
    } else {
        println!("  (none)");
    }

    // --- Memory / Backend ---
    output.print_header("Memory / Backend");
    let backend = match &config.memory.session_backend {
        config_core::model::SessionBackend::Sqlite => "sqlite",
        config_core::model::SessionBackend::Sled => "sled",
        config_core::model::SessionBackend::Postgres => "postgres",
    };
    output.print_kv("session_backend", backend);
    output.print_kv(
        "qdrant",
        if config.memory.qdrant_enabled {
            config.memory.qdrant_url.as_deref().unwrap_or("enabled")
        } else {
            "disabled"
        },
    );
    output.print_kv(
        "postgres_url",
        &opt_str(config.memory.postgres_url.as_deref()),
    );
    output.print_kv(
        "obsidian_vault",
        &opt_str(
            config
                .memory
                .obsidian_vault_path
                .as_deref()
                .and_then(|p| p.to_str()),
        ),
    );

    output.print_blank();

    // --- Event Log ---
    diag_event_log(output).await;

    Ok(())
}

async fn diag_json(config: &Config, output: &Output, executor: &Executor) -> CliResult<()> {
    let cap_provider = config
        .global
        .default_provider
        .clone()
        .unwrap_or_else(|| ProviderId::new("default"));

    let capabilities = executor.capabilities(cap_provider.clone()).await.ok();

    let auth_statuses: Vec<serde_json::Value> = {
        let probes: Vec<ProviderId> = if config.providers.is_empty() {
            vec![cap_provider]
        } else {
            config.providers.iter().map(|p| p.id.clone()).collect()
        };
        let mut out = Vec::new();
        for pid in probes {
            match executor.auth_status(pid.clone()).await {
                Ok(info) => out.push(
                    serde_json::to_value(&info)
                        .map_err(|e| CliError::Other(e.to_string()))?,
                ),
                Err(e) => out.push(serde_json::json!({
                    "provider_id": pid.0,
                    "error": e.to_string(),
                })),
            }
        }
        out
    };

    let sessions = executor.session_list()?;

    let data = serde_json::json!({
        "global": {
            "default_provider": config.global.default_provider.as_ref().map(|p| &p.0),
            "default_model": config.global.default_model.as_ref().map(|m| &m.0),
            "log_level": config.global.log_level.as_deref().unwrap_or(&config.logging.level),
            "max_context_tokens": config.global.max_context_tokens,
        },
        "cli": {
            "color": config.cli.color,
            "pager": config.cli.pager,
        },
        "provider_count": config.providers.len(),
        "auth_statuses": auth_statuses,
        "capabilities": capabilities,
        "session_count": sessions.len(),
        "memory": {
            "qdrant_enabled": config.memory.qdrant_enabled,
        }
    });

    output.print_success("", &data);
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Print event-log diagnostics section.
async fn diag_event_log(output: &Output) {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let path = std::path::PathBuf::from(&home).join(".rustpi/events.jsonl");

    output.print_header("Event Log");

    if path.exists() {
        match FileEventStore::open(&path).await {
            Ok(store) => {
                let reader = ReplayReader::from_file_tolerant(&store).await;
                let failures = reader.recent_failures(5);
                let incomplete = reader.incomplete_runs();
                output.print_kv("log_path", &path.display().to_string());
                output.print_kv("total_events", &reader.all().len().to_string());

                output.print_kv(
                    "recent_failures",
                    &if failures.is_empty() {
                        "none".to_string()
                    } else {
                        failures
                            .iter()
                            .map(|r| {
                                serde_json::to_value(&r.event)
                                    .ok()
                                    .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(String::from))
                                    .unwrap_or_else(|| "<unknown>".to_string())
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    },
                );
                output.print_kv(
                    "incomplete_runs",
                    &if incomplete.is_empty() {
                        "none".to_string()
                    } else {
                        format!(
                            "{} run(s): {}",
                            incomplete.len(),
                            incomplete
                                .iter()
                                .map(|r| format!("{} ({:?})", &r.run_id[..8], r.state))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    },
                );
            }
            Err(e) => {
                output.print_kv("status", &format!("error reading log: {}", e));
            }
        }
    } else {
        output.print_kv("status", "No event log connected");
        output.print_kv("hint", "run `rustpi replay` to inspect a log file");
    }
}

fn opt_str(s: Option<&str>) -> String {
    s.unwrap_or("-").to_string()
}

fn provider_kind_str(kind: &ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenAiCompatible => "openai_compatible",
        ProviderKind::LlamaCpp => "llama_cpp",
        ProviderKind::Vllm => "vllm",
        ProviderKind::GithubCopilot => "github_copilot",
        ProviderKind::Gemini => "gemini",
    }
}

fn provider_auth_str(auth: &ProviderAuthConfig) -> String {
    match auth {
        ProviderAuthConfig::ApiKey { env_var } => format!("api_key({})", env_var),
        ProviderAuthConfig::OAuthBrowser => "oauth_browser".to_string(),
        ProviderAuthConfig::DeviceCode => "device_code".to_string(),
    }
}

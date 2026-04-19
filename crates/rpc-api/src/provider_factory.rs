//! Factory for building a [`ProviderRegistry`] from a loaded [`Config`].

use std::sync::Arc;

use auth_core::{ApiKeyAuth, AuthFlow, AuthState, MemoryTokenStore};
use auth_core::record::TokenRecord;
use chrono::Utc;
use config_core::model::{Config, ProviderAuthConfig, ProviderKind};
use model_adapters::{
    CopilotAdapter, CopilotConfig, LlamaCppAdapter, LlamaCppConfig, OpenAiAdapter, OpenAiConfig,
    ProviderRegistry, VllmAdapter, VllmConfig,
    adapters::echo::EchoProvider,
};
use tracing::warn;

/// Build a [`ProviderRegistry`] from the provider list in `config`.
///
/// - API key resolution failures emit a warning but do not abort startup;
///   the provider is still registered and will return an auth error at inference time.
/// - `Gemini` providers are skipped with a warning (not yet implemented).
/// - If no providers are configured, a built-in echo provider is registered so
///   the CLI always produces output without requiring an API key.
pub fn build_provider_registry(config: &Config) -> ProviderRegistry {
    let mut registry = ProviderRegistry::new();

    for provider_cfg in &config.providers {
        let id = &provider_cfg.id;

        match &provider_cfg.kind {
            ProviderKind::OpenAiCompatible => {
                let api_key = resolve_api_key(&id.0, &provider_cfg.auth);
                let openai_cfg = OpenAiConfig {
                    provider_id: id.clone(),
                    base_url: provider_cfg
                        .base_url
                        .clone()
                        .unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
                    api_key,
                    extra_headers: vec![],
                    supports_embeddings: true,
                    supports_model_discovery: true,
                    static_models: vec![],
                    timeout_secs: 120,
                };
                match OpenAiAdapter::new(openai_cfg) {
                    Ok(adapter) => {
                        registry.register(Arc::new(adapter));
                    }
                    Err(e) => {
                        warn!(provider = %id, error = %e, "failed to create OpenAiAdapter, skipping");
                    }
                }
            }

            ProviderKind::LlamaCpp => {
                let llamacpp_cfg = LlamaCppConfig {
                    provider_id: id.clone(),
                    base_url: provider_cfg
                        .base_url
                        .clone()
                        .unwrap_or_else(|| "http://localhost:8080/v1".to_string()),
                    ..LlamaCppConfig::default()
                };
                match LlamaCppAdapter::new(llamacpp_cfg) {
                    Ok(adapter) => {
                        registry.register(Arc::new(adapter));
                    }
                    Err(e) => {
                        warn!(provider = %id, error = %e, "failed to create LlamaCppAdapter, skipping");
                    }
                }
            }

            ProviderKind::Vllm => {
                let api_key = resolve_api_key(&id.0, &provider_cfg.auth);
                let vllm_cfg = VllmConfig {
                    provider_id: id.clone(),
                    base_url: provider_cfg
                        .base_url
                        .clone()
                        .unwrap_or_else(|| "http://localhost:8000/v1".to_string()),
                    api_key,
                    ..VllmConfig::default()
                };
                match VllmAdapter::new(vllm_cfg) {
                    Ok(adapter) => {
                        registry.register(Arc::new(adapter));
                    }
                    Err(e) => {
                        warn!(provider = %id, error = %e, "failed to create VllmAdapter, skipping");
                    }
                }
            }

            ProviderKind::GithubCopilot => {
                let copilot_cfg = CopilotConfig {
                    provider_id: id.clone(),
                    base_url: provider_cfg
                        .base_url
                        .clone()
                        .unwrap_or_else(|| "https://api.githubcopilot.com".to_string()),
                    ..CopilotConfig::default()
                };
                match CopilotAdapter::new(copilot_cfg) {
                    Ok(adapter) => {
                        registry.register(Arc::new(adapter));
                    }
                    Err(e) => {
                        warn!(provider = %id, error = %e, "failed to create CopilotAdapter, skipping");
                    }
                }
            }

            ProviderKind::Gemini => {
                warn!(provider = %id, "Gemini provider not yet implemented, skipping");
            }
        }
    }

    // Register echo fallback when no real providers are configured so the
    // CLI always works without an API key.
    if registry.is_empty() {
        registry.register(Arc::new(EchoProvider::new()));
        tracing::debug!("no providers configured — registered built-in echo fallback");
    }

    registry
}

/// Build a [`MemoryTokenStore`] pre-populated with `Authenticated` states for
/// every provider whose API key was successfully resolved from the environment.
pub fn build_token_store_for_config(config: &Config) -> MemoryTokenStore {
    let store = MemoryTokenStore::new();

    for provider_cfg in &config.providers {
        if let ProviderAuthConfig::ApiKey { env_var } = &provider_cfg.auth {
            let auth_helper = ApiKeyAuth::new(provider_cfg.id.clone());
            match auth_helper.resolve_key(Some(env_var.as_str()), None) {
                Ok(record) => {
                    let token_record = TokenRecord {
                        provider_id: provider_cfg.id.clone(),
                        access_token: record.access_token,
                        refresh_token: None,
                        expires_at: None, // API keys don't expire
                        scopes: vec![],
                        flow: AuthFlow::ApiKey,
                        stored_at: Utc::now(),
                    };
                    store.save_record(token_record);
                }
                Err(_) => {
                    // Key not found — leave this provider unauthenticated in the store.
                }
            }
        }
    }

    store
}

/// Resolve an API key from the provider's auth config.
/// Returns `None` on failure and emits a warning so startup continues.
fn resolve_api_key(provider_id: &str, auth: &ProviderAuthConfig) -> Option<String> {
    match auth {
        ProviderAuthConfig::ApiKey { env_var } => {
            let auth_helper = ApiKeyAuth::new(agent_core::types::ProviderId::new(provider_id));
            match auth_helper.resolve_key(Some(env_var.as_str()), None) {
                Ok(record) => Some(record.access_token),
                Err(e) => {
                    warn!(
                        provider = %provider_id,
                        env_var = %env_var,
                        error = %e,
                        "API key not found; provider registered but will fail at inference time"
                    );
                    None
                }
            }
        }
        ProviderAuthConfig::OAuthBrowser | ProviderAuthConfig::DeviceCode => None,
    }
}

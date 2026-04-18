//! API key authentication path.

use chrono::Utc;

use crate::error::AuthError;
use crate::record::TokenRecord;
use crate::{AuthFlow, AuthState, ProviderId};

// ── ApiKeyAuth ─────────────────────────────────────────────────────────────

/// Builds an `AuthState` and `TokenRecord` from a plaintext API key.
pub struct ApiKeyAuth {
    pub provider_id: ProviderId,
}

impl ApiKeyAuth {
    pub fn new(provider_id: ProviderId) -> Self {
        Self { provider_id }
    }

    /// Resolve the API key.
    ///
    /// Priority:
    /// 1. Environment variable named by `env_var` (if provided and set).
    /// 2. `config_value` (if provided).
    /// 3. Returns `AuthError::NoKeyAvailable` otherwise.
    pub fn resolve_key(
        &self,
        env_var: Option<&str>,
        config_value: Option<&str>,
    ) -> Result<TokenRecord, AuthError> {
        let key = if let Some(var) = env_var {
            std::env::var(var)
                .ok()
                .filter(|v| !v.is_empty())
                .or_else(|| config_value.map(str::to_string))
        } else {
            config_value.map(str::to_string)
        };

        let access_token = key.ok_or_else(|| AuthError::NoKeyAvailable {
            env_var: env_var.map(str::to_string),
        })?;

        Ok(TokenRecord {
            provider_id: self.provider_id.clone(),
            access_token,
            refresh_token: None,
            expires_at: None, // API keys don't expire (until manually revoked)
            scopes: vec![],
            flow: AuthFlow::ApiKey,
            stored_at: Utc::now(),
        })
    }

    /// Build an `AuthState` from a token record.
    /// API keys are always `Authenticated` with no expiry.
    pub fn auth_state(record: &TokenRecord) -> AuthState {
        AuthState::Authenticated {
            provider: record.provider_id.clone(),
            expires_at: None,
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn auth() -> ApiKeyAuth {
        ApiKeyAuth::new(ProviderId::new("openai"))
    }

    #[test]
    fn resolve_key_error_when_no_env_and_no_config() {
        let result = auth().resolve_key(None, None);
        assert!(matches!(result, Err(AuthError::NoKeyAvailable { .. })));
    }

    #[test]
    fn resolve_key_error_when_env_not_set_and_no_config() {
        // Use an env var that definitely isn't set.
        let result = auth().resolve_key(Some("__RUSTPI_NONEXISTENT_VAR__"), None);
        assert!(matches!(result, Err(AuthError::NoKeyAvailable { .. })));
    }

    #[test]
    fn resolve_key_returns_record_from_config_value() {
        let rec = auth().resolve_key(None, Some("my-api-key")).unwrap();
        assert_eq!(rec.access_token, "my-api-key");
        assert!(matches!(rec.flow, AuthFlow::ApiKey));
        assert!(rec.expires_at.is_none());
    }

    #[test]
    fn resolve_key_prefers_env_var_over_config() {
        // Set a temporary env var.
        std::env::set_var("__RUSTPI_TEST_API_KEY__", "env-key");
        let rec = auth()
            .resolve_key(Some("__RUSTPI_TEST_API_KEY__"), Some("config-key"))
            .unwrap();
        std::env::remove_var("__RUSTPI_TEST_API_KEY__");
        assert_eq!(rec.access_token, "env-key");
    }

    #[test]
    fn resolve_key_falls_back_to_config_when_env_not_set() {
        let result =
            auth().resolve_key(Some("__RUSTPI_NONEXISTENT_VAR__"), Some("fallback-key"));
        let rec = result.unwrap();
        assert_eq!(rec.access_token, "fallback-key");
    }

    #[test]
    fn auth_state_returns_authenticated_with_no_expiry() {
        let rec = auth().resolve_key(None, Some("key")).unwrap();
        let state = ApiKeyAuth::auth_state(&rec);
        match state {
            AuthState::Authenticated { expires_at, .. } => {
                assert!(expires_at.is_none());
            }
            _ => panic!("expected Authenticated"),
        }
    }
}

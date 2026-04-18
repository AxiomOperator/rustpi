//! OAuth 2.0 authorization code flow with optional PKCE.

use chrono::Utc;
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::error::AuthError;
use crate::record::TokenRecord;
use crate::{AuthFlow, ProviderId};

// ── Public types ──────────────────────────────────────────────────────────

/// Configuration for an OAuth authorization code flow.
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub provider_id: ProviderId,
    pub client_id: String,
    /// None for PKCE-only (public-client) flows.
    pub client_secret: Option<String>,
    pub auth_url: String,
    pub token_url: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
}

/// State for an in-progress OAuth flow.
#[derive(Debug, Clone)]
pub struct OAuthPendingState {
    /// CSRF state parameter.
    pub state_token: String,
    pub pkce_verifier: Option<String>,
    pub authorization_url: String,
}

/// Outcome of a completed OAuth token exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
    pub scope: Option<String>,
    pub token_type: String,
}

// ── OAuthFlow ─────────────────────────────────────────────────────────────

/// OAuth browser flow helper.
pub struct OAuthFlow {
    config: OAuthConfig,
}

impl OAuthFlow {
    pub fn new(config: OAuthConfig) -> Self {
        Self { config }
    }

    /// Generate the authorization URL and CSRF state.
    /// Returns the pending state (including the URL to visit).
    pub fn begin(&self) -> Result<OAuthPendingState, AuthError> {
        // 16-byte random state → 32 hex chars
        let mut bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut bytes);
        let state_token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();

        let scope = self.config.scopes.join(" ");
        let mut url = format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}",
            self.config.auth_url,
            urlencoded(&self.config.client_id),
            urlencoded(&self.config.redirect_uri),
            urlencoded(&scope),
            urlencoded(&state_token),
        );

        // Append any extra provider-specific parameters here in future phases.
        let _ = &mut url; // suppress unused-mut lint

        Ok(OAuthPendingState {
            state_token,
            pkce_verifier: None,
            authorization_url: url,
        })
    }

    /// Exchange an authorization code for tokens.
    /// Validates the CSRF state parameter first.
    pub async fn exchange_code(
        &self,
        code: &str,
        returned_state: &str,
        pending: &OAuthPendingState,
    ) -> Result<OAuthTokenResponse, AuthError> {
        // CSRF check
        if returned_state != pending.state_token {
            return Err(AuthError::CsrfMismatch {
                expected: pending.state_token.clone(),
                got: returned_state.to_string(),
            });
        }

        let client = reqwest::Client::new();
        let mut params = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code.to_string()),
            ("redirect_uri", self.config.redirect_uri.clone()),
            ("client_id", self.config.client_id.clone()),
        ];
        if let Some(secret) = &self.config.client_secret {
            params.push(("client_secret", secret.clone()));
        }

        let resp = client
            .post(&self.config.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| AuthError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AuthError::HttpError(format!(
                "token exchange failed: {status} — {body}"
            )));
        }

        let token_resp: OAuthTokenResponse = resp
            .json()
            .await
            .map_err(|e| AuthError::HttpError(e.to_string()))?;

        Ok(token_resp)
    }

    /// Build a `TokenRecord` from an `OAuthTokenResponse`.
    pub fn into_token_record(&self, response: OAuthTokenResponse) -> TokenRecord {
        let expires_at = response.expires_in.map(|secs| {
            Utc::now() + chrono::Duration::seconds(secs as i64)
        });
        let scopes = response
            .scope
            .as_deref()
            .unwrap_or("")
            .split_whitespace()
            .map(str::to_string)
            .collect();

        TokenRecord {
            provider_id: self.config.provider_id.clone(),
            access_token: response.access_token,
            refresh_token: response.refresh_token,
            expires_at,
            scopes,
            flow: AuthFlow::OAuthBrowser,
            stored_at: Utc::now(),
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Minimal URL percent-encoding of common characters.
fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(c),
            ' ' => out.push('+'),
            c => {
                let mut buf = [0u8; 4];
                let encoded = c.encode_utf8(&mut buf);
                for byte in encoded.bytes() {
                    out.push_str(&format!("%{byte:02X}"));
                }
            }
        }
    }
    out
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> OAuthConfig {
        OAuthConfig {
            provider_id: ProviderId::new("github"),
            client_id: "client123".into(),
            client_secret: Some("secret".into()),
            auth_url: "https://github.com/login/oauth/authorize".into(),
            token_url: "https://github.com/login/oauth/access_token".into(),
            redirect_uri: "http://localhost:8080/callback".into(),
            scopes: vec!["repo".into(), "read:user".into()],
        }
    }

    #[test]
    fn begin_generates_unique_state_tokens() {
        let flow = OAuthFlow::new(sample_config());
        let p1 = flow.begin().unwrap();
        let p2 = flow.begin().unwrap();
        assert_ne!(p1.state_token, p2.state_token);
    }

    #[test]
    fn begin_url_contains_state_and_client_id() {
        let flow = OAuthFlow::new(sample_config());
        let pending = flow.begin().unwrap();
        assert!(
            pending.authorization_url.contains("client_id=client123"),
            "URL missing client_id"
        );
        assert!(
            pending.authorization_url.contains(&pending.state_token),
            "URL missing state token"
        );
    }

    #[test]
    fn begin_url_contains_redirect_uri() {
        let flow = OAuthFlow::new(sample_config());
        let pending = flow.begin().unwrap();
        assert!(
            pending.authorization_url.contains("redirect_uri="),
            "URL missing redirect_uri"
        );
    }

    #[tokio::test]
    async fn exchange_code_fails_on_csrf_mismatch() {
        let flow = OAuthFlow::new(sample_config());
        let pending = flow.begin().unwrap();
        let result = flow
            .exchange_code("authcode", "wrong-state", &pending)
            .await;
        assert!(matches!(result, Err(AuthError::CsrfMismatch { .. })));
    }
}

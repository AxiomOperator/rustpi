//! Device authorization flow (RFC 8628).

use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::error::AuthError;
use crate::oauth::OAuthTokenResponse;
use crate::ProviderId;

// ── Public types ──────────────────────────────────────────────────────────

/// Configuration for a device authorization flow.
#[derive(Debug, Clone)]
pub struct DeviceFlowConfig {
    pub provider_id: ProviderId,
    pub client_id: String,
    pub device_auth_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
}

/// The device code response from the provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    /// Polling interval in seconds.
    pub interval: u64,
}

#[derive(Debug, Clone)]
pub enum DeviceFlowResult {
    Success(OAuthTokenResponse),
    Expired,
    Cancelled,
}

// ── DeviceFlow ────────────────────────────────────────────────────────────

/// Device authorization flow implementation.
pub struct DeviceFlow {
    config: DeviceFlowConfig,
}

impl DeviceFlow {
    pub fn new(config: DeviceFlowConfig) -> Self {
        Self { config }
    }

    /// Request a device code from the provider.
    pub async fn request_device_code(&self) -> Result<DeviceCodeResponse, AuthError> {
        let client = reqwest::Client::new();
        let scope = self.config.scopes.join(" ");
        let params = [
            ("client_id", self.config.client_id.as_str()),
            ("scope", scope.as_str()),
        ];

        let resp = client
            .post(&self.config.device_auth_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| AuthError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AuthError::HttpError(format!(
                "device code request failed: {status} — {body}"
            )));
        }

        let device_code: DeviceCodeResponse = resp
            .json()
            .await
            .map_err(|e| AuthError::HttpError(e.to_string()))?;

        Ok(device_code)
    }

    /// Poll the token endpoint until the user authorizes, the flow expires, or `cancel_token` fires.
    pub async fn poll_for_token(
        &self,
        device_code: &DeviceCodeResponse,
        cancel_token: CancellationToken,
    ) -> Result<DeviceFlowResult, AuthError> {
        let client = reqwest::Client::new();
        let mut interval_secs = device_code.interval;

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    return Ok(DeviceFlowResult::Cancelled);
                }
                _ = sleep(std::time::Duration::from_secs(interval_secs)) => {}
            }

            let params = [
                (
                    "grant_type",
                    "urn:ietf:params:oauth:grant-type:device_code",
                ),
                ("device_code", device_code.device_code.as_str()),
                ("client_id", self.config.client_id.as_str()),
            ];

            let resp = client
                .post(&self.config.token_url)
                .form(&params)
                .send()
                .await
                .map_err(|e| AuthError::HttpError(e.to_string()))?;

            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();

            if status.is_success() {
                let token_resp: OAuthTokenResponse =
                    serde_json::from_str(&body)?;
                return Ok(DeviceFlowResult::Success(token_resp));
            }

            // RFC 8628 §3.5 error responses
            let error_code = extract_error_code(&body);
            match error_code.as_deref() {
                Some("authorization_pending") => {
                    // Continue polling.
                }
                Some("slow_down") => {
                    interval_secs += 5;
                }
                Some("expired_token") | Some("access_denied") => {
                    return Ok(DeviceFlowResult::Expired);
                }
                _ => {
                    return Err(AuthError::HttpError(format!(
                        "device flow poll failed: {status} — {body}"
                    )));
                }
            }
        }
    }
}

/// Extract the `error` field from a JSON or form-encoded error response.
fn extract_error_code(body: &str) -> Option<String> {
    // Try JSON first.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(e) = v.get("error").and_then(|e| e.as_str()) {
            return Some(e.to_string());
        }
    }
    // Fall back to form-encoded: `error=authorization_pending&...`
    for part in body.split('&') {
        if let Some(val) = part.strip_prefix("error=") {
            return Some(val.to_string());
        }
    }
    None
}

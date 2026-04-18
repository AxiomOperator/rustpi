//! Token refresh coordinator.

use chrono::Utc;

use crate::error::AuthError;
use crate::record::TokenRecord;

/// How far in advance to consider a token "needs refresh" (5 minutes).
const REFRESH_MARGIN: std::time::Duration = std::time::Duration::from_secs(300);

/// Returns true if the token should be refreshed (expires within 5 minutes or is already expired).
pub fn needs_refresh(record: &TokenRecord) -> bool {
    record.expires_within(REFRESH_MARGIN)
}

/// Refresh an access token using the stored refresh_token.
/// POSTs to `token_url` and returns an updated `TokenRecord`.
pub async fn refresh_token(
    record: &TokenRecord,
    token_url: &str,
    client_id: &str,
    client_secret: Option<&str>,
) -> Result<TokenRecord, AuthError> {
    let refresh_tok = record
        .refresh_token
        .as_deref()
        .ok_or_else(|| AuthError::NoRefreshToken(record.provider_id.clone()))?;

    let client = reqwest::Client::new();
    let mut params = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_tok.to_string()),
        ("client_id", client_id.to_string()),
    ];
    if let Some(secret) = client_secret {
        params.push(("client_secret", secret.to_string()));
    }

    let resp = client
        .post(token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| AuthError::HttpError(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AuthError::RefreshFailed(format!(
            "{status} — {body}"
        )));
    }

    #[derive(serde::Deserialize)]
    struct RefreshResponse {
        access_token: String,
        refresh_token: Option<String>,
        expires_in: Option<u64>,
        scope: Option<String>,
    }

    let parsed: RefreshResponse = resp
        .json()
        .await
        .map_err(|e| AuthError::HttpError(e.to_string()))?;

    let expires_at = parsed
        .expires_in
        .map(|secs| Utc::now() + chrono::Duration::seconds(secs as i64));

    let scopes: Vec<String> = parsed
        .scope
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_string)
        .collect();

    Ok(TokenRecord {
        provider_id: record.provider_id.clone(),
        access_token: parsed.access_token,
        // Use new refresh token if provider rotated it; otherwise keep old one.
        refresh_token: parsed.refresh_token.or_else(|| record.refresh_token.clone()),
        expires_at,
        scopes: if scopes.is_empty() { record.scopes.clone() } else { scopes },
        flow: record.flow.clone(),
        stored_at: Utc::now(),
    })
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuthFlow, ProviderId};
    use chrono::Utc;

    fn make_record(expires_at: Option<chrono::DateTime<Utc>>) -> TokenRecord {
        TokenRecord {
            provider_id: ProviderId::new("openai"),
            access_token: "tok".into(),
            refresh_token: Some("ref".into()),
            expires_at,
            scopes: vec![],
            flow: AuthFlow::OAuthBrowser,
            stored_at: Utc::now(),
        }
    }

    #[test]
    fn needs_refresh_true_for_token_expiring_in_2_minutes() {
        let r = make_record(Some(Utc::now() + chrono::Duration::minutes(2)));
        assert!(needs_refresh(&r));
    }

    #[test]
    fn needs_refresh_false_for_token_expiring_in_10_minutes() {
        let r = make_record(Some(Utc::now() + chrono::Duration::minutes(10)));
        assert!(!needs_refresh(&r));
    }

    #[test]
    fn needs_refresh_false_for_token_with_no_expiry() {
        let r = make_record(None);
        assert!(!needs_refresh(&r));
    }
}

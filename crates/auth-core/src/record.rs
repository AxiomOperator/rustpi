//! Token record model — a persisted auth token for one provider.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{AuthFlow, AuthState, ProviderId};

/// A persisted auth token record for one provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRecord {
    pub provider_id: ProviderId,
    /// The access token (bearer token or API key).
    pub access_token: String,
    /// Optional refresh token.
    pub refresh_token: Option<String>,
    /// When this token expires. None = does not expire.
    pub expires_at: Option<DateTime<Utc>>,
    /// OAuth scopes granted.
    pub scopes: Vec<String>,
    /// The auth flow that produced this token.
    pub flow: AuthFlow,
    /// When this record was stored.
    pub stored_at: DateTime<Utc>,
}

impl TokenRecord {
    /// Returns true if the token is expired (or expires within the given margin).
    pub fn is_expired(&self) -> bool {
        self.expires_within(std::time::Duration::ZERO)
    }

    /// Returns true if the token expires within `margin` duration.
    pub fn expires_within(&self, margin: std::time::Duration) -> bool {
        match self.expires_at {
            None => false,
            Some(exp) => {
                let margin = chrono::Duration::from_std(margin).unwrap_or_default();
                Utc::now() + margin >= exp
            }
        }
    }

    /// Returns true if this record has a refresh token available.
    pub fn can_refresh(&self) -> bool {
        self.refresh_token.is_some()
    }

    /// Convert to AuthState.
    pub fn to_auth_state(&self) -> AuthState {
        if self.is_expired() {
            AuthState::Expired {
                provider: self.provider_id.clone(),
            }
        } else {
            AuthState::Authenticated {
                provider: self.provider_id.clone(),
                expires_at: self.expires_at,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(expires_at: Option<DateTime<Utc>>, refresh_token: Option<String>) -> TokenRecord {
        TokenRecord {
            provider_id: ProviderId::new("test"),
            access_token: "tok".into(),
            refresh_token,
            expires_at,
            scopes: vec![],
            flow: AuthFlow::ApiKey,
            stored_at: Utc::now(),
        }
    }

    #[test]
    fn is_expired_false_for_future() {
        let r = make_record(
            Some(Utc::now() + chrono::Duration::hours(1)),
            None,
        );
        assert!(!r.is_expired());
    }

    #[test]
    fn is_expired_true_for_past() {
        let r = make_record(
            Some(Utc::now() - chrono::Duration::seconds(1)),
            None,
        );
        assert!(r.is_expired());
    }

    #[test]
    fn expires_within_5min_when_token_expires_in_3min() {
        let r = make_record(
            Some(Utc::now() + chrono::Duration::minutes(3)),
            None,
        );
        assert!(r.expires_within(std::time::Duration::from_secs(300)));
    }

    #[test]
    fn expires_within_not_triggered_for_far_future() {
        let r = make_record(
            Some(Utc::now() + chrono::Duration::hours(2)),
            None,
        );
        assert!(!r.expires_within(std::time::Duration::from_secs(300)));
    }

    #[test]
    fn can_refresh_true_when_refresh_token_present() {
        let r = make_record(None, Some("refresh".into()));
        assert!(r.can_refresh());
    }

    #[test]
    fn can_refresh_false_when_no_refresh_token() {
        let r = make_record(None, None);
        assert!(!r.can_refresh());
    }

    #[test]
    fn to_auth_state_authenticated_for_non_expired() {
        let r = make_record(
            Some(Utc::now() + chrono::Duration::hours(1)),
            None,
        );
        assert!(matches!(r.to_auth_state(), AuthState::Authenticated { .. }));
    }

    #[test]
    fn to_auth_state_authenticated_for_no_expiry() {
        let r = make_record(None, None);
        assert!(matches!(r.to_auth_state(), AuthState::Authenticated { .. }));
    }

    #[test]
    fn to_auth_state_expired_for_past_token() {
        let r = make_record(
            Some(Utc::now() - chrono::Duration::seconds(1)),
            None,
        );
        assert!(matches!(r.to_auth_state(), AuthState::Expired { .. }));
    }
}

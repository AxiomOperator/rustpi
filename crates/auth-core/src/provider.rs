//! Provider auth trait. Implemented per-provider in `model-adapters`.
//!
//! Phase 0 stub — full implementations in Phase 3.

use crate::{AuthError, AuthState, ProviderId};

/// Trait for provider-specific authentication operations.
///
/// Each provider adapter implements this to expose login, refresh, and revoke.
#[async_trait::async_trait]
pub trait ProviderAuth: Send + Sync {
    fn provider_id(&self) -> &ProviderId;
    async fn login(&self) -> Result<AuthState, AuthError>;
    async fn refresh(&self) -> Result<AuthState, AuthError>;
    async fn revoke(&self) -> Result<(), AuthError>;
    async fn current_state(&self) -> AuthState;
}

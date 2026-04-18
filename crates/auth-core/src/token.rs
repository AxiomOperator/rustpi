//! Token storage abstraction.
//!
//! Tokens are stored encrypted at rest. The storage backend is configurable.
//! Phase 0 stub — encryption and persistence deferred to Phase 3.

use crate::{AuthError, AuthState, ProviderId};

/// Stores and retrieves auth tokens for providers.
#[async_trait::async_trait]
pub trait TokenStore: Send + Sync {
    async fn load(&self, provider: &ProviderId) -> Result<Option<AuthState>, AuthError>;
    async fn save(&self, provider: &ProviderId, state: &AuthState) -> Result<(), AuthError>;
    async fn delete(&self, provider: &ProviderId) -> Result<(), AuthError>;
}

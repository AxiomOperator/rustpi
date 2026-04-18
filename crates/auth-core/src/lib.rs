//! Auth subsystem supporting OAuth browser flow, device authorization flow,
//! and API key authentication. Token storage is encrypted at rest.
//!
//! # Flows
//! - [`AuthFlow::OAuthBrowser`] — opens browser for user consent
//! - [`AuthFlow::DeviceCode`] — displays device code for headless environments
//! - [`AuthFlow::ApiKey`] — static key stored in config

pub mod api_key;
pub mod device_flow;
pub mod encrypted_store;
pub mod error;
pub mod oauth;
pub mod provider;
pub mod record;
pub mod refresh;
pub mod token;

pub use agent_core::types::{AuthFlow, AuthState, ProviderId};
pub use api_key::ApiKeyAuth;
pub use device_flow::{DeviceCodeResponse, DeviceFlow, DeviceFlowConfig, DeviceFlowResult};
pub use encrypted_store::{EncryptedFileTokenStore, MemoryTokenStore};
pub use error::AuthError;
pub use oauth::{OAuthConfig, OAuthFlow, OAuthPendingState, OAuthTokenResponse};
pub use record::TokenRecord;
pub use refresh::{needs_refresh, refresh_token};
pub use token::TokenStore;

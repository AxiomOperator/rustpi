//! Auth subsystem supporting OAuth browser flow, device authorization flow,
//! and API key authentication. Token storage is encrypted at rest.
//!
//! # Flows
//! - [`AuthFlow::OAuthBrowser`] — opens browser for user consent
//! - [`AuthFlow::DeviceCode`] — displays device code for headless environments
//! - [`AuthFlow::ApiKey`] — static key stored in config
//!
//! # Status
//! Phase 0 stub — interfaces defined, implementations deferred to Phase 3.

pub mod error;
pub mod provider;
pub mod token;

pub use agent_core::types::{AuthFlow, AuthState, ProviderId};
pub use error::AuthError;

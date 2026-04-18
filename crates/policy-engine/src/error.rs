use thiserror::Error;

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("action denied by policy rule '{rule}': {reason}")]
    Denied { rule: String, reason: String },
    #[error("policy configuration invalid: {0}")]
    ConfigInvalid(String),
}

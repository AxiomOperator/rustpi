use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file not found: {0}")]
    NotFound(String),
    #[error("config parse error: {0}")]
    Parse(String),
    #[error("missing required config key: {0}")]
    MissingKey(String),
    #[error("invalid value for {key}: {reason}")]
    InvalidValue { key: String, reason: String },
}

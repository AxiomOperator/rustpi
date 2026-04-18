//! Factory functions for constructing store instances from `MemoryConfig`.

use crate::{
    postgres::PostgresBackend,
    sled_store::SledBackend,
    sqlite::SqliteBackend,
    store::{MemoryStore, RunStore, SessionStore, SummaryStore},
    StoreError,
};
use config_core::model::{MemoryConfig, SessionBackend};
use std::sync::Arc;

fn default_sqlite_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    format!("sqlite://{home}/.rustpi/sessions.db")
}

fn default_sled_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    format!("{home}/.rustpi/sessions.sled")
}

pub async fn build_session_store(config: &MemoryConfig) -> Result<Arc<dyn SessionStore>, StoreError> {
    match &config.session_backend {
        SessionBackend::Sqlite => {
            let path = default_sqlite_path();
            Ok(Arc::new(SqliteBackend::connect(&path).await?))
        }
        SessionBackend::Sled => {
            let path = default_sled_path();
            Ok(Arc::new(SledBackend::open(&path)?))
        }
        SessionBackend::Postgres => {
            let url = config
                .postgres_url
                .as_deref()
                .ok_or_else(|| StoreError::Connection("postgres_url not configured".into()))?;
            Ok(Arc::new(PostgresBackend::connect(url).await?))
        }
    }
}

pub async fn build_run_store(config: &MemoryConfig) -> Result<Arc<dyn RunStore>, StoreError> {
    match &config.session_backend {
        SessionBackend::Sqlite => {
            let path = default_sqlite_path();
            Ok(Arc::new(SqliteBackend::connect(&path).await?))
        }
        SessionBackend::Sled => {
            let path = default_sled_path();
            Ok(Arc::new(SledBackend::open(&path)?))
        }
        SessionBackend::Postgres => {
            let url = config
                .postgres_url
                .as_deref()
                .ok_or_else(|| StoreError::Connection("postgres_url not configured".into()))?;
            Ok(Arc::new(PostgresBackend::connect(url).await?))
        }
    }
}

pub async fn build_summary_store(config: &MemoryConfig) -> Result<Arc<dyn SummaryStore>, StoreError> {
    match &config.session_backend {
        SessionBackend::Sqlite => {
            let path = default_sqlite_path();
            Ok(Arc::new(SqliteBackend::connect(&path).await?))
        }
        SessionBackend::Sled => {
            let path = default_sled_path();
            Ok(Arc::new(SledBackend::open(&path)?))
        }
        SessionBackend::Postgres => {
            let url = config
                .postgres_url
                .as_deref()
                .ok_or_else(|| StoreError::Connection("postgres_url not configured".into()))?;
            Ok(Arc::new(PostgresBackend::connect(url).await?))
        }
    }
}

pub async fn build_memory_store(config: &MemoryConfig) -> Result<Arc<dyn MemoryStore>, StoreError> {
    match &config.session_backend {
        SessionBackend::Sqlite => {
            let path = default_sqlite_path();
            Ok(Arc::new(SqliteBackend::connect(&path).await?))
        }
        SessionBackend::Sled => {
            let path = default_sled_path();
            Ok(Arc::new(SledBackend::open(&path)?))
        }
        SessionBackend::Postgres => {
            let url = config
                .postgres_url
                .as_deref()
                .ok_or_else(|| StoreError::Connection("postgres_url not configured".into()))?;
            Ok(Arc::new(PostgresBackend::connect(url).await?))
        }
    }
}

//! Encrypted file-backed token store.
//!
//! # Security model
//! The 256-bit AES-GCM key is stored in `{config_dir}/rustpi/auth.key` (base64-encoded),
//! and encrypted token data lives in `{config_dir}/rustpi/tokens.enc`.
//!
//! This provides encryption-at-rest against bulk data copies but **not** against an attacker
//! with full filesystem access (who could read both the key and the ciphertext).
//! Platform keyring integration is deferred to a later phase.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

use crate::error::AuthError;
use crate::record::TokenRecord;
use crate::token::TokenStore;
use crate::{AuthState, ProviderId};

// ── Encrypted file store ───────────────────────────────────────────────────

/// Encrypted file-backed token store.
pub struct EncryptedFileTokenStore {
    tokens_path: PathBuf,
    key_path: PathBuf,
}

impl EncryptedFileTokenStore {
    /// Open or create the store at `{config_dir}/rustpi/`.
    /// Generates a new encryption key if one doesn't exist yet.
    pub fn new(config_dir: impl Into<PathBuf>) -> Result<Self, AuthError> {
        let base: PathBuf = config_dir.into().join("rustpi");
        std::fs::create_dir_all(&base)?;

        let key_path = base.join("auth.key");
        let tokens_path = base.join("tokens.enc");

        // Ensure key exists (generates one if not).
        Self::load_or_create_key(&key_path)?;

        Ok(Self {
            tokens_path,
            key_path,
        })
    }

    // ── Private helpers ────────────────────────────────────────────────────

    fn load_or_create_key(key_path: &Path) -> Result<[u8; 32], AuthError> {
        if key_path.exists() {
            let b64 = std::fs::read_to_string(key_path)?;
            let bytes = B64
                .decode(b64.trim())
                .map_err(|e| AuthError::DecryptionError(format!("bad key file: {e}")))?;
            if bytes.len() != 32 {
                return Err(AuthError::DecryptionError(
                    "key file has wrong length".into(),
                ));
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            Ok(key)
        } else {
            // Generate a fresh key.
            let key = Aes256Gcm::generate_key(OsRng);
            let b64 = B64.encode(key.as_slice());
            std::fs::write(key_path, b64)?;
            let mut out = [0u8; 32];
            out.copy_from_slice(key.as_slice());
            Ok(out)
        }
    }

    fn cipher(&self) -> Result<Aes256Gcm, AuthError> {
        let raw = Self::load_or_create_key(&self.key_path)?;
        let key = Key::<Aes256Gcm>::from_slice(&raw);
        Ok(Aes256Gcm::new(key))
    }

    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, AuthError> {
        let cipher = self.cipher()?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng); // 96-bit nonce
        let ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|e| AuthError::EncryptionError(e.to_string()))?;
        // Prepend nonce (12 bytes) to ciphertext.
        let mut out = Vec::with_capacity(nonce.len() + ciphertext.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, AuthError> {
        if data.len() < 12 {
            return Err(AuthError::DecryptionError("ciphertext too short".into()));
        }
        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
        let cipher = self.cipher()?;
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| AuthError::DecryptionError(e.to_string()))
    }

    fn read_all(&self) -> Result<HashMap<String, TokenRecord>, AuthError> {
        if !self.tokens_path.exists() {
            return Ok(HashMap::new());
        }
        let raw = std::fs::read(&self.tokens_path)?;
        let plaintext = self.decrypt(&raw)?;
        let map: HashMap<String, TokenRecord> = serde_json::from_slice(&plaintext)?;
        Ok(map)
    }

    fn write_all(&self, records: &HashMap<String, TokenRecord>) -> Result<(), AuthError> {
        let plaintext = serde_json::to_vec(records)?;
        let ciphertext = self.encrypt(&plaintext)?;
        std::fs::write(&self.tokens_path, ciphertext)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl TokenStore for EncryptedFileTokenStore {
    async fn load(&self, provider: &ProviderId) -> Result<Option<AuthState>, AuthError> {
        let map = self.read_all()?;
        Ok(map.get(&provider.0).map(|r| r.to_auth_state()))
    }

    async fn save(&self, provider: &ProviderId, state: &AuthState) -> Result<(), AuthError> {
        // state must be Authenticated to produce a meaningful record;
        // callers should use save_record for full fidelity.
        // For the trait contract we store whatever we can reconstruct.
        let _ = (provider, state);
        Err(AuthError::Storage(
            "use save_record for EncryptedFileTokenStore".into(),
        ))
    }

    async fn delete(&self, provider: &ProviderId) -> Result<(), AuthError> {
        let mut map = self.read_all()?;
        map.remove(&provider.0);
        self.write_all(&map)
    }
}

impl EncryptedFileTokenStore {
    /// Save a full `TokenRecord` (preferred over the trait's `save`).
    pub fn save_record(&self, record: &TokenRecord) -> Result<(), AuthError> {
        let mut map = self.read_all()?;
        map.insert(record.provider_id.0.clone(), record.clone());
        self.write_all(&map)
    }

    /// Load a full `TokenRecord`.
    pub fn load_record(
        &self,
        provider: &ProviderId,
    ) -> Result<Option<TokenRecord>, AuthError> {
        let map = self.read_all()?;
        Ok(map.get(&provider.0).cloned())
    }
}

// ── In-memory store (for tests / ephemeral sessions) ──────────────────────

/// In-memory token store — useful for tests and ephemeral sessions.
#[derive(Default, Clone)]
pub struct MemoryTokenStore {
    records: Arc<Mutex<HashMap<String, TokenRecord>>>,
}

impl MemoryTokenStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn save_record(&self, record: TokenRecord) {
        self.records
            .lock()
            .unwrap()
            .insert(record.provider_id.0.clone(), record);
    }

    pub fn load_record(&self, provider: &ProviderId) -> Option<TokenRecord> {
        self.records.lock().unwrap().get(&provider.0).cloned()
    }
}

#[async_trait::async_trait]
impl TokenStore for MemoryTokenStore {
    async fn load(&self, provider: &ProviderId) -> Result<Option<AuthState>, AuthError> {
        Ok(self
            .records
            .lock()
            .unwrap()
            .get(&provider.0)
            .map(|r| r.to_auth_state()))
    }

    async fn save(&self, provider: &ProviderId, state: &AuthState) -> Result<(), AuthError> {
        use crate::{AuthFlow, AuthState};
        // Build a minimal TokenRecord from AuthState.
        let record = match state {
            AuthState::Authenticated { provider: p, expires_at } => TokenRecord {
                provider_id: p.clone(),
                access_token: String::new(),
                refresh_token: None,
                expires_at: *expires_at,
                scopes: vec![],
                flow: AuthFlow::ApiKey,
                stored_at: chrono::Utc::now(),
            },
            _ => {
                // For non-Authenticated states, remove the record.
                self.records.lock().unwrap().remove(&provider.0);
                return Ok(());
            }
        };
        self.records
            .lock()
            .unwrap()
            .insert(provider.0.clone(), record);
        Ok(())
    }

    async fn delete(&self, provider: &ProviderId) -> Result<(), AuthError> {
        self.records.lock().unwrap().remove(&provider.0);
        Ok(())
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuthFlow, AuthState, ProviderId};
    use chrono::Utc;

    fn sample_record(provider: &str) -> TokenRecord {
        TokenRecord {
            provider_id: ProviderId::new(provider),
            access_token: "access-123".into(),
            refresh_token: Some("refresh-456".into()),
            expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
            scopes: vec!["read".into()],
            flow: AuthFlow::OAuthBrowser,
            stored_at: Utc::now(),
        }
    }

    // ── MemoryTokenStore ───────────────────────────────────────────────────

    #[tokio::test]
    async fn memory_save_load_roundtrip() {
        let store = MemoryTokenStore::new();
        let rec = sample_record("openai");
        store.save_record(rec.clone());
        let loaded = store.load_record(&ProviderId::new("openai")).unwrap();
        assert_eq!(loaded.access_token, "access-123");
    }

    #[tokio::test]
    async fn memory_load_nonexistent_returns_none() {
        let store = MemoryTokenStore::new();
        assert!(store.load_record(&ProviderId::new("ghost")).is_none());
    }

    #[tokio::test]
    async fn memory_delete_removes_record() {
        let store = MemoryTokenStore::new();
        let rec = sample_record("openai");
        store.save_record(rec);
        store
            .delete(&ProviderId::new("openai"))
            .await
            .unwrap();
        assert!(store.load_record(&ProviderId::new("openai")).is_none());
    }

    #[tokio::test]
    async fn memory_load_trait_returns_auth_state() {
        let store = MemoryTokenStore::new();
        let rec = sample_record("openai");
        store.save_record(rec);
        let state = store.load(&ProviderId::new("openai")).await.unwrap();
        assert!(matches!(state, Some(AuthState::Authenticated { .. })));
    }

    // ── EncryptedFileTokenStore ────────────────────────────────────────────

    #[test]
    fn encrypted_key_file_created_on_first_use() {
        let dir = tempfile::tempdir().unwrap();
        let store = EncryptedFileTokenStore::new(dir.path()).unwrap();
        let key_path = dir.path().join("rustpi").join("auth.key");
        assert!(key_path.exists(), "key file should be created");
        drop(store);
    }

    #[test]
    fn encrypted_save_then_load_returns_same_record() {
        let dir = tempfile::tempdir().unwrap();
        let store = EncryptedFileTokenStore::new(dir.path()).unwrap();
        let rec = sample_record("github");
        store.save_record(&rec).unwrap();
        let loaded = store.load_record(&ProviderId::new("github")).unwrap().unwrap();
        assert_eq!(loaded.access_token, rec.access_token);
        assert_eq!(loaded.refresh_token, rec.refresh_token);
    }

    #[test]
    fn encrypted_tokens_survive_reopen() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = EncryptedFileTokenStore::new(dir.path()).unwrap();
            store.save_record(&sample_record("github")).unwrap();
        }
        // Open a new instance at the same path.
        let store2 = EncryptedFileTokenStore::new(dir.path()).unwrap();
        let loaded = store2
            .load_record(&ProviderId::new("github"))
            .unwrap()
            .unwrap();
        assert_eq!(loaded.access_token, "access-123");
    }

    #[test]
    fn encrypted_delete_removes_record() {
        let dir = tempfile::tempdir().unwrap();
        let store = EncryptedFileTokenStore::new(dir.path()).unwrap();
        store.save_record(&sample_record("github")).unwrap();

        // Use a blocking delete via a tiny runtime.
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(store.delete(&ProviderId::new("github")))
            .unwrap();

        assert!(
            store
                .load_record(&ProviderId::new("github"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn encrypted_load_nonexistent_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = EncryptedFileTokenStore::new(dir.path()).unwrap();
        assert!(store.load_record(&ProviderId::new("nobody")).unwrap().is_none());
    }

    // -----------------------------------------------------------------------
    // Phase 12 chaos / failure tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn chaos_memory_store_load_missing_provider_returns_ok_none() {
        // Loading an unknown provider from MemoryTokenStore must return Ok(None), not an error.
        let store = MemoryTokenStore::new();
        let result = store.load(&ProviderId::new("nonexistent-provider")).await;
        assert!(result.is_ok(), "load should succeed for unknown provider");
        assert!(result.unwrap().is_none(), "load should return None for unknown provider");
    }

    #[tokio::test]
    async fn chaos_memory_store_expired_token_returns_expired_state() {
        // A token that is already expired should surface as AuthState::Expired, not panic.
        let store = MemoryTokenStore::new();
        let expired_record = TokenRecord {
            provider_id: ProviderId::new("openai"),
            access_token: "expired-tok".into(),
            refresh_token: None,
            expires_at: Some(Utc::now() - chrono::Duration::hours(1)),
            scopes: vec![],
            flow: AuthFlow::OAuthBrowser,
            stored_at: Utc::now(),
        };
        store.save_record(expired_record);

        let state = store.load(&ProviderId::new("openai")).await.unwrap();
        assert!(
            matches!(state, Some(AuthState::Expired { .. })),
            "expired token must surface as AuthState::Expired, got: {state:?}"
        );
    }

    #[tokio::test]
    async fn chaos_memory_store_empty_access_token_stores_without_panic() {
        // An empty access token must be stored and retrieved without panicking.
        let store = MemoryTokenStore::new();
        let record = TokenRecord {
            provider_id: ProviderId::new("test-provider"),
            access_token: String::new(), // deliberately empty
            refresh_token: None,
            expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
            scopes: vec![],
            flow: AuthFlow::ApiKey,
            stored_at: Utc::now(),
        };
        store.save_record(record);
        let loaded = store.load_record(&ProviderId::new("test-provider"));
        assert!(loaded.is_some(), "record with empty token should be stored");
        assert_eq!(loaded.unwrap().access_token, "", "access_token should remain empty");
    }

    #[tokio::test]
    async fn chaos_memory_store_delete_nonexistent_does_not_error() {
        // Deleting a provider that was never stored must succeed gracefully.
        let store = MemoryTokenStore::new();
        let result = store.delete(&ProviderId::new("ghost-provider")).await;
        assert!(result.is_ok(), "deleting a nonexistent provider should not return an error");
    }
}

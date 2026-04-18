//! Runtime registry of available provider instances.

use crate::provider::ModelProvider;
use agent_core::types::ProviderId;
use std::collections::HashMap;
use std::sync::Arc;

/// A runtime registry of available provider instances.
/// Providers are registered by ID and retrieved by ID.
pub struct ProviderRegistry {
    providers: HashMap<ProviderId, Arc<dyn ModelProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Register a provider. If a provider with the same ID already exists, it is replaced.
    pub fn register(&mut self, provider: Arc<dyn ModelProvider>) {
        let id = provider.provider_id().clone();
        self.providers.insert(id, provider);
    }

    /// Retrieve a provider by ID.
    pub fn get(&self, id: &ProviderId) -> Option<&Arc<dyn ModelProvider>> {
        self.providers.get(id)
    }

    /// List all registered provider IDs.
    pub fn list(&self) -> Vec<&ProviderId> {
        self.providers.keys().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

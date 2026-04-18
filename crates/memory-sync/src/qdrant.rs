//! Qdrant vector memory backend.
//!
//! Stores memory records as points in a Qdrant collection, with content and
//! metadata in the payload. Phase 7 does not have an embedding model, so
//! `retrieve` uses scroll with keyword filtering rather than ANN search.

use crate::error::MemorySyncError;
use crate::memory::MemoryRecord;
use agent_core::types::SessionId;
use async_trait::async_trait;
use chrono::Utc;
use context_engine::memory::{MemoryQuery, MemoryRetriever};
use context_engine::packer::MemorySnippet;
use context_engine::tokens;
use qdrant_client::qdrant::{
    CreateCollectionBuilder, Distance, ScrollPointsBuilder, UpsertPointsBuilder,
    VectorParamsBuilder,
};
use qdrant_client::{Payload, Qdrant};
use tracing::warn;
use uuid::Uuid;

pub const DEFAULT_COLLECTION: &str = "rustpi_memory";
pub const DEFAULT_VECTOR_SIZE: u64 = 1536;

/// Qdrant-backed memory store.
pub struct QdrantMemory {
    client: Qdrant,
    collection: String,
    vector_size: u64,
}

impl QdrantMemory {
    /// Create a new `QdrantMemory` connecting to `url`.
    ///
    /// `collection` defaults to [`DEFAULT_COLLECTION`].
    /// `vector_size` defaults to [`DEFAULT_VECTOR_SIZE`].
    pub fn new(
        url: &str,
        collection: Option<String>,
        vector_size: Option<u64>,
    ) -> Result<Self, MemorySyncError> {
        let client = Qdrant::from_url(url)
            .build()
            .map_err(|e| MemorySyncError::Qdrant(e.to_string()))?;
        Ok(Self {
            client,
            collection: collection.unwrap_or_else(|| DEFAULT_COLLECTION.to_string()),
            vector_size: vector_size.unwrap_or(DEFAULT_VECTOR_SIZE),
        })
    }

    /// Ensure the collection exists; creates it if not.
    pub async fn ensure_collection(&self) -> Result<(), MemorySyncError> {
        let exists = self
            .client
            .collection_exists(&self.collection)
            .await
            .map_err(|e| MemorySyncError::Qdrant(e.to_string()))?;
        if !exists {
            self.client
                .create_collection(
                    CreateCollectionBuilder::new(&self.collection).vectors_config(
                        VectorParamsBuilder::new(self.vector_size, Distance::Cosine),
                    ),
                )
                .await
                .map_err(|e| MemorySyncError::Qdrant(e.to_string()))?;
        }
        Ok(())
    }

    /// Upsert a memory record with its embedding into Qdrant.
    pub async fn upsert_memory(
        &self,
        record: &MemoryRecord,
        embedding: Vec<f32>,
    ) -> Result<(), MemorySyncError> {
        let mut payload = Payload::new();
        payload.insert("content", record.content.clone());
        payload.insert(
            "tags",
            serde_json::to_string(&record.tags)
                .unwrap_or_else(|_| "[]".to_string()),
        );
        if let Some(sid) = &record.session_id {
            payload.insert("session_id", sid.to_string());
        }
        payload.insert("created_at", record.created_at.to_rfc3339());
        payload.insert("updated_at", record.updated_at.to_rfc3339());

        let point =
            qdrant_client::qdrant::PointStruct::new(record.id.to_string(), embedding, payload);

        self.client
            .upsert_points(UpsertPointsBuilder::new(&self.collection, vec![point]).wait(true))
            .await
            .map_err(|e| MemorySyncError::Qdrant(e.to_string()))?;
        Ok(())
    }

    /// Search for similar memories by embedding vector. Returns up to `limit` records.
    pub async fn search_similar(
        &self,
        embedding: Vec<f32>,
        limit: usize,
    ) -> Result<Vec<MemoryRecord>, MemorySyncError> {
        use qdrant_client::qdrant::SearchPointsBuilder;
        let results = self
            .client
            .search_points(
                SearchPointsBuilder::new(&self.collection, embedding, limit as u64)
                    .with_payload(true),
            )
            .await
            .map_err(|e| MemorySyncError::Qdrant(e.to_string()))?;

        let records = results
            .result
            .into_iter()
            .filter_map(|point| scored_point_to_record(point).ok())
            .collect();
        Ok(records)
    }

    /// Keyword-based retrieval via scroll (Phase 7 — no embeddings available).
    async fn retrieve_by_keywords(
        &self,
        query: &MemoryQuery,
    ) -> Result<Vec<MemorySnippet>, MemorySyncError> {
        let scroll = ScrollPointsBuilder::new(&self.collection)
            .limit(256)
            .with_payload(true);

        let response = self
            .client
            .scroll(scroll)
            .await
            .map_err(|e| MemorySyncError::Qdrant(e.to_string()))?;

        let mut snippets = Vec::new();
        let mut total_tokens = 0u32;

        for point in response.result {
            if snippets.len() >= query.max_snippets {
                break;
            }
            if total_tokens >= query.total_token_budget {
                break;
            }

            let content = match extract_str(&point.payload, "content") {
                Some(c) => c,
                None => continue,
            };

            let matches = query.keywords.is_empty()
                || query.keywords.iter().any(|kw| {
                    content.to_lowercase().contains(&kw.to_lowercase())
                });
            if !matches {
                continue;
            }

            // Session filter
            if let Some(sid) = &query.session_id {
                if let Some(point_sid) = extract_str(&point.payload, "session_id") {
                    if &point_sid != sid {
                        continue;
                    }
                }
            }

            let snip_tokens = tokens::estimate(&content);
            if snip_tokens > query.max_tokens_per_snippet {
                continue;
            }
            if total_tokens + snip_tokens > query.total_token_budget {
                break;
            }

            total_tokens += snip_tokens;
            snippets.push(MemorySnippet {
                source: "qdrant".to_string(),
                content,
                tokens: snip_tokens,
            });
        }

        Ok(snippets)
    }
}

#[async_trait]
impl MemoryRetriever for QdrantMemory {
    async fn retrieve(&self, query: &MemoryQuery) -> Vec<MemorySnippet> {
        match self.retrieve_by_keywords(query).await {
            Ok(snippets) => snippets,
            Err(e) => {
                warn!("QdrantMemory retrieve failed: {e}");
                vec![]
            }
        }
    }
}

fn extract_str(
    payload: &std::collections::HashMap<String, qdrant_client::qdrant::Value>,
    key: &str,
) -> Option<String> {
    use qdrant_client::qdrant::value::Kind;
    payload.get(key)?.kind.as_ref().and_then(|k| {
        if let Kind::StringValue(s) = k {
            Some(s.clone())
        } else {
            None
        }
    })
}

fn scored_point_to_record(
    point: qdrant_client::qdrant::ScoredPoint,
) -> Result<MemoryRecord, MemorySyncError> {
    let content = extract_str(&point.payload, "content")
        .ok_or_else(|| MemorySyncError::Qdrant("missing content in payload".into()))?;
    let tags_str = extract_str(&point.payload, "tags").unwrap_or_else(|| "[]".to_string());
    let tags: Vec<String> =
        serde_json::from_str(&tags_str).unwrap_or_default();
    let session_id = extract_str(&point.payload, "session_id").and_then(|s| {
        Uuid::parse_str(&s).ok().map(SessionId)
    });
    let now = Utc::now();
    Ok(MemoryRecord {
        id: Uuid::new_v4(),
        session_id,
        content,
        tags,
        embedding: None,
        created_at: now,
        updated_at: now,
    })
}

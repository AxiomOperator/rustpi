# ADR-0002: Provider Abstraction

**Status:** Accepted

## Context

`rustpi` must support multiple model providers with different capabilities, auth flows, and API shapes:
- OpenAI-compatible APIs (OpenAI, Azure OpenAI, local vLLM, llama.cpp)
- OAuth/device-auth providers (GitHub Copilot, Gemini)
- Local providers with no network auth

The system must route requests to the correct adapter, normalize errors, and expose capability metadata uniformly.

## Decision

Define a `ModelProvider` trait in `model-adapters::provider` that all provider adapters implement:

```rust
#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn provider_id(&self) -> &ProviderId;
    fn capabilities(&self, model: &ModelId) -> ProviderCapabilities;
    async fn list_models(&self) -> Result<Vec<ModelId>, ProviderError>;
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, ProviderError>;
    async fn complete_stream(...) -> Result<Pin<Box<dyn Stream<Item = Result<TokenDelta, ProviderError>> + Send>>, ProviderError>;
    async fn embed(&self, inputs: Vec<String>) -> Result<Vec<Vec<f32>>, ProviderError>;
    async fn auth_state(&self) -> AuthState;
}
```

Key decisions:
1. **Errors normalize at the adapter boundary.** Provider-specific HTTP/SDK errors become `ProviderError` variants before leaving the adapter. `agent-core` never sees raw HTTP status codes.
2. **Streaming uses `futures::Stream`.** This is idiomatic Rust async and composes with `tokio-stream`.
3. **Auth state is queried, not pushed.** Adapters expose `auth_state()` synchronously; the `auth-core` subsystem handles token refresh out-of-band and emits `AuthStateChanged` events.
4. **Capabilities are per-model, not per-provider.** A provider may host models with different context windows, tool-calling support, and vision capabilities.
5. **`ProviderId` and `ModelId` are typed wrappers** over `String` to prevent accidental confusion at call sites.

Provider adapters are registered in a `ProviderRegistry` (Phase 4) and selected by `ProviderId` at run time.

## Consequences

**Positive:**
- Adding a new provider requires only implementing `ModelProvider`
- `agent-core` is fully decoupled from provider HTTP details
- Capability negotiation is explicit and machine-readable

**Negative:**
- The trait is async and object-safe only with `async_trait` (dyn dispatch overhead)
- Streaming return type requires boxing, which adds a small allocation per call
- Embedding support is optional per provider; callers must check `ProviderCapabilities::embeddings`

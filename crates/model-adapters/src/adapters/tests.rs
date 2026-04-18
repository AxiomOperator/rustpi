#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::{
        ProviderError,
        adapters::{
            copilot::{CopilotAdapter, CopilotConfig},
            llamacpp::{LlamaCppAdapter, LlamaCppConfig},
            openai::{OpenAiAdapter, OpenAiConfig},
        },
        provider::{ChatMessage, CompletionRequest, EmbeddingRequest, FinishReason, MessageContent, ModelProvider, Role},
    };
    use agent_core::types::{ModelId, ProviderId};

    // ─── Helper builders ────────────────────────────────────────────────────────

    fn chat_response_body(content: &str) -> String {
        serde_json::json!({
            "id": "chatcmpl-test",
            "model": "gpt-4o",
            "choices": [{
                "message": {"role": "assistant", "content": content},
                "finish_reason": "stop",
                "index": 0
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        })
        .to_string()
    }

    fn model_list_body(ids: &[&str]) -> String {
        let data: Vec<_> = ids
            .iter()
            .map(|id| serde_json::json!({"id": id, "owned_by": "openai"}))
            .collect();
        serde_json::json!({"object": "list", "data": data}).to_string()
    }

    fn embedding_response_body(n: usize, dims: usize) -> String {
        let data: Vec<_> = (0..n)
            .map(|i| {
                serde_json::json!({
                    "embedding": vec![0.1f32; dims],
                    "index": i
                })
            })
            .collect();
        serde_json::json!({
            "data": data,
            "model": "text-embedding-3-small",
            "usage": {"prompt_tokens": 5, "completion_tokens": 0, "total_tokens": 5}
        })
        .to_string()
    }

    fn openai_error_body(message: &str, kind: &str) -> String {
        serde_json::json!({
            "error": {"message": message, "type": kind, "code": null}
        })
        .to_string()
    }

    fn simple_request() -> CompletionRequest {
        CompletionRequest {
            model: ModelId::new("gpt-4o"),
            messages: vec![ChatMessage {
                role: Role::User,
                content: MessageContent::Text("hello".into()),
            }],
            max_tokens: Some(10),
            temperature: None,
            tools: vec![],
        }
    }

    fn openai_adapter(server: &MockServer) -> OpenAiAdapter {
        OpenAiAdapter::new(OpenAiConfig {
            provider_id: ProviderId::new("openai"),
            base_url: server.uri(),
            api_key: Some("test-key".to_string()),
            extra_headers: vec![],
            supports_embeddings: true,
            supports_model_discovery: true,
            static_models: vec![],
            timeout_secs: 10,
        })
        .unwrap()
    }

    // ─── 1. Model discovery — OpenAI adapter ────────────────────────────────────

    #[tokio::test]
    async fn openai_list_models_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(model_list_body(&["gpt-4o", "gpt-4"]))
                    .insert_header("content-type", "application/json"),
            )
            .mount(&server)
            .await;

        let adapter = openai_adapter(&server);
        let models = adapter.list_models().await.unwrap();
        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|m| m.id.to_string() == "gpt-4o"));
        assert!(models.iter().any(|m| m.id.to_string() == "gpt-4"));
    }

    #[tokio::test]
    async fn openai_list_models_auth_failure() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_string(openai_error_body("Invalid API key", "invalid_request_error"))
                    .insert_header("content-type", "application/json"),
            )
            .mount(&server)
            .await;

        let adapter = openai_adapter(&server);
        let result = adapter.list_models().await;
        assert!(matches!(result, Err(ProviderError::Unauthorized(_))));
    }

    #[tokio::test]
    async fn openai_list_models_server_unavailable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(
                ResponseTemplate::new(503)
                    .set_body_string("Service Unavailable")
                    .insert_header("content-type", "text/plain"),
            )
            .mount(&server)
            .await;

        let adapter = openai_adapter(&server);
        let result = adapter.list_models().await;
        assert!(
            matches!(result, Err(ProviderError::Unavailable(_)))
                || matches!(result, Err(ProviderError::ApiError { .. }))
        );
    }

    // ─── 2. Non-streaming chat — OpenAI adapter ─────────────────────────────────

    #[tokio::test]
    async fn openai_complete_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(chat_response_body("Hello from the model!"))
                    .insert_header("content-type", "application/json"),
            )
            .mount(&server)
            .await;

        let adapter = openai_adapter(&server);
        let response = adapter.complete(simple_request()).await.unwrap();
        if let MessageContent::Text(t) = &response.message.content {
            assert_eq!(t, "Hello from the model!");
        } else {
            panic!("expected Text content");
        }
        assert_eq!(response.usage.prompt_tokens, 10);
        assert_eq!(response.usage.completion_tokens, 5);
        assert_eq!(response.usage.total_tokens, 15);
    }

    #[tokio::test]
    async fn openai_complete_rate_limited() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429).set_body_string("rate limit exceeded"))
            .mount(&server)
            .await;

        let adapter = openai_adapter(&server);
        let result = adapter.complete(simple_request()).await;
        assert!(matches!(result, Err(ProviderError::RateLimited { .. })));
    }

    #[tokio::test]
    async fn openai_complete_invalid_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(422)
                    .set_body_string(openai_error_body("Invalid request", "invalid_request_error"))
                    .insert_header("content-type", "application/json"),
            )
            .mount(&server)
            .await;

        let adapter = openai_adapter(&server);
        let result = adapter.complete(simple_request()).await;
        assert!(matches!(result, Err(ProviderError::InvalidRequest(_))));
    }

    #[tokio::test]
    async fn openai_complete_malformed_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("this is not json at all {{{{")
                    .insert_header("content-type", "application/json"),
            )
            .mount(&server)
            .await;

        let adapter = openai_adapter(&server);
        let result = adapter.complete(simple_request()).await;
        assert!(matches!(result, Err(ProviderError::MalformedResponse(_))));
    }

    #[tokio::test]
    async fn openai_complete_empty_choices() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "id": "chatcmpl-empty",
            "model": "gpt-4o",
            "choices": [],
            "usage": {"prompt_tokens": 5, "completion_tokens": 0, "total_tokens": 5}
        })
        .to_string();
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(body)
                    .insert_header("content-type", "application/json"),
            )
            .mount(&server)
            .await;

        let adapter = openai_adapter(&server);
        let result = adapter.complete(simple_request()).await;
        match result {
            Err(ProviderError::MalformedResponse(msg)) => {
                assert!(msg.contains("empty choices"), "expected 'empty choices' in: {msg}");
            }
            other => panic!("expected MalformedResponse, got {other:?}"),
        }
    }

    // ─── 3. Streaming chat — OpenAI adapter ─────────────────────────────────────

    #[tokio::test]
    async fn openai_complete_stream_success() {
        let server = MockServer::start().await;
        let sse_body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\" world\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_body)
                    .insert_header("content-type", "text/event-stream"),
            )
            .mount(&server)
            .await;

        let adapter = openai_adapter(&server);
        let mut stream = adapter.complete_stream(simple_request()).await.unwrap();

        let mut deltas = vec![];
        while let Some(item) = stream.next().await {
            deltas.push(item.unwrap());
        }

        assert!(!deltas.is_empty(), "expected at least one delta");
        assert_eq!(deltas[0].text, Some("Hello".to_string()));
        // Second delta has finish_reason stop
        assert!(
            deltas.iter().any(|d| matches!(d.finish_reason, Some(FinishReason::Stop))),
            "expected a Stop finish reason among deltas"
        );
    }

    #[tokio::test]
    async fn openai_complete_stream_malformed_chunks_are_skipped() {
        let server = MockServer::start().await;
        // One well-formed chunk, one malformed, one well-formed with DONE
        let sse_body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Valid\"},\"finish_reason\":null}]}\n\n",
            "data: {not valid json at all}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"OK\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_body)
                    .insert_header("content-type", "text/event-stream"),
            )
            .mount(&server)
            .await;

        let adapter = openai_adapter(&server);
        let stream = adapter.complete_stream(simple_request()).await.unwrap();
        let results: Vec<_> = stream.collect().await;

        // All items that are Ok should have valid text
        let ok_deltas: Vec<_> = results.into_iter().filter_map(|r| r.ok()).collect();
        assert!(
            ok_deltas.iter().any(|d| d.text == Some("Valid".to_string())),
            "expected 'Valid' delta"
        );
        assert!(
            ok_deltas.iter().any(|d| d.text == Some("OK".to_string())),
            "expected 'OK' delta"
        );
    }

    // ─── 4. Embeddings — OpenAI adapter ─────────────────────────────────────────

    #[tokio::test]
    async fn openai_embed_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/embeddings"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(embedding_response_body(2, 64))
                    .insert_header("content-type", "application/json"),
            )
            .mount(&server)
            .await;

        let adapter = openai_adapter(&server);
        let response = adapter
            .embed(EmbeddingRequest {
                model: ModelId::new("text-embedding-3-small"),
                inputs: vec!["hello".to_string(), "world".to_string()],
                dimensions: None,
            })
            .await
            .unwrap();

        assert_eq!(response.embeddings.len(), 2);
        assert_eq!(response.embeddings[0].len(), 64);
        assert_eq!(response.model.to_string(), "text-embedding-3-small");
    }

    #[tokio::test]
    async fn openai_embed_unavailable_when_disabled() {
        // No mock needed — adapter must short-circuit before any HTTP call
        let adapter = OpenAiAdapter::new(OpenAiConfig {
            provider_id: ProviderId::new("openai"),
            base_url: "http://127.0.0.1:1".to_string(), // unreachable port
            api_key: Some("key".to_string()),
            extra_headers: vec![],
            supports_embeddings: false,
            supports_model_discovery: true,
            static_models: vec![],
            timeout_secs: 5,
        })
        .unwrap();

        let result = adapter
            .embed(EmbeddingRequest {
                model: ModelId::new("text-embedding-3-small"),
                inputs: vec!["test".to_string()],
                dimensions: None,
            })
            .await;
        assert!(matches!(result, Err(ProviderError::UnsupportedCapability(_))));
    }

    // ─── 5. Error normalisation ──────────────────────────────────────────────────

    #[tokio::test]
    async fn error_normalization_401_maps_to_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_string(openai_error_body("Bad credentials", "invalid_request_error"))
                    .insert_header("content-type", "application/json"),
            )
            .mount(&server)
            .await;

        let adapter = openai_adapter(&server);
        let result = adapter.complete(simple_request()).await;
        match result {
            Err(ProviderError::Unauthorized(msg)) => {
                assert!(msg.contains("Bad credentials"), "unexpected msg: {msg}");
            }
            other => panic!("expected Unauthorized, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn error_normalization_403_maps_to_forbidden() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(403)
                    .set_body_string(openai_error_body("Forbidden", "forbidden"))
                    .insert_header("content-type", "application/json"),
            )
            .mount(&server)
            .await;

        let adapter = openai_adapter(&server);
        let result = adapter.complete(simple_request()).await;
        assert!(matches!(result, Err(ProviderError::Forbidden(_))));
    }

    #[tokio::test]
    async fn error_normalization_429_maps_to_rate_limited() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429).set_body_string("too many requests"))
            .mount(&server)
            .await;

        let adapter = openai_adapter(&server);
        let result = adapter.complete(simple_request()).await;
        assert!(matches!(result, Err(ProviderError::RateLimited { .. })));
    }

    #[tokio::test]
    async fn error_normalization_500_maps_to_unavailable() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal server error"))
            .mount(&server)
            .await;

        let adapter = openai_adapter(&server);
        let result = adapter.complete(simple_request()).await;
        assert!(matches!(result, Err(ProviderError::Unavailable(_))));
    }

    // ─── 6. llama.cpp adapter — server not available fallback ───────────────────

    #[tokio::test]
    async fn llamacpp_list_models_fallback_when_server_down() {
        // Port 19999 is expected to have nothing listening.
        let config = LlamaCppConfig {
            base_url: "http://127.0.0.1:19999".to_string(),
            static_models: vec![],
            timeout_secs: 2,
            ..Default::default()
        };
        let adapter = LlamaCppAdapter::new(config).unwrap();
        let models = adapter.list_models().await.unwrap();
        assert!(
            models.is_empty(),
            "expected empty fallback list, got {models:?}"
        );
    }

    // ─── 7. Copilot adapter ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn copilot_list_models_returns_static_list() {
        let adapter = CopilotAdapter::new(CopilotConfig::default()).unwrap();
        let models = adapter.list_models().await.unwrap();
        assert!(!models.is_empty());
        assert!(
            models.iter().any(|m| m.id.to_string().contains("gpt-4")),
            "expected at least one gpt-4 model"
        );
    }

    #[tokio::test]
    async fn copilot_auth_required_when_no_token() {
        let adapter = CopilotAdapter::new(CopilotConfig::default()).unwrap();
        let result = adapter.complete(simple_request()).await;
        assert!(
            matches!(result, Err(ProviderError::AuthRequired(_))),
            "expected AuthRequired, got {result:?}"
        );
    }

    #[tokio::test]
    async fn copilot_complete_with_token_calls_api() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(chat_response_body("Copilot response"))
                    .insert_header("content-type", "application/json"),
            )
            .mount(&server)
            .await;

        let config = CopilotConfig {
            copilot_token: Some("copilot-test-token".to_string()),
            base_url: server.uri(),
            timeout_secs: 10,
            ..Default::default()
        };
        let adapter = CopilotAdapter::new(config).unwrap();
        let response = adapter.complete(simple_request()).await.unwrap();

        if let MessageContent::Text(t) = &response.message.content {
            assert_eq!(t, "Copilot response");
        } else {
            panic!("expected Text content");
        }
    }
}

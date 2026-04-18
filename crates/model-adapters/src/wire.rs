// OpenAI-compatible request/response wire types for HTTP serialization.
// These normalize the JSON shapes used by OpenAI, llama.cpp, vLLM, GitHub Copilot, etc.

use serde::{Deserialize, Serialize};

/// Wire format for a chat message sent to OpenAI-compatible APIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireChatMessage {
    pub role: String,
    pub content: serde_json::Value, // string or array of content parts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<WireToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String, // "function"
    pub function: WireFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireFunction {
    pub name: String,
    pub arguments: String, // JSON string
}

/// Chat completion request body.
#[derive(Debug, Clone, Serialize)]
pub struct WireChatRequest {
    pub model: String,
    pub messages: Vec<WireChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<serde_json::Value>,
}

/// Non-streaming chat completion response.
#[derive(Debug, Clone, Deserialize)]
pub struct WireChatResponse {
    pub id: String,
    pub choices: Vec<WireChoice>,
    pub usage: Option<WireUsage>,
    pub model: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WireChoice {
    pub message: WireChatMessage,
    pub finish_reason: Option<String>,
    pub index: u32,
}

/// Streaming chunk.
#[derive(Debug, Clone, Deserialize)]
pub struct WireStreamChunk {
    pub choices: Vec<WireStreamChoice>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WireStreamChoice {
    pub delta: WireDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WireDelta {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<WireToolCall>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WireUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Embedding request body.
#[derive(Debug, Clone, Serialize)]
pub struct WireEmbeddingRequest {
    pub model: String,
    pub input: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<u32>,
}

/// Embedding response.
#[derive(Debug, Clone, Deserialize)]
pub struct WireEmbeddingResponse {
    pub data: Vec<WireEmbeddingData>,
    pub usage: WireUsage,
    pub model: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WireEmbeddingData {
    pub embedding: Vec<f32>,
    pub index: u32,
}

/// Model list response.
#[derive(Debug, Clone, Deserialize)]
pub struct WireModelList {
    pub data: Vec<WireModelObject>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WireModelObject {
    pub id: String,
    #[serde(default)]
    pub owned_by: String,
}

/// OpenAI-style API error body.
#[derive(Debug, Clone, Deserialize)]
pub struct WireErrorBody {
    pub error: WireErrorDetail,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WireErrorDetail {
    pub message: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub code: Option<String>,
}

/// Convert an OpenAI wire error response into a ProviderError.
pub fn map_api_error(status: u16, body: &str) -> crate::ProviderError {
    let message = if let Ok(err) = serde_json::from_str::<WireErrorBody>(body) {
        err.error.message
    } else {
        body.to_string()
    };

    match status {
        401 => crate::ProviderError::Unauthorized(message),
        403 => crate::ProviderError::Forbidden(message),
        404 => crate::ProviderError::ApiError { status, message },
        422 => crate::ProviderError::InvalidRequest(message),
        429 => crate::ProviderError::RateLimited { retry_after_secs: 60 },
        500..=503 => crate::ProviderError::Unavailable(message),
        _ => crate::ProviderError::ApiError { status, message },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_api_error_401() {
        let body = r#"{"error":{"message":"Invalid API key","type":"invalid_request_error","code":"invalid_api_key"}}"#;
        let err = map_api_error(401, body);
        assert!(matches!(err, crate::ProviderError::Unauthorized(_)));
        if let crate::ProviderError::Unauthorized(msg) = err {
            assert!(msg.contains("Invalid API key"));
        }
    }

    #[test]
    fn map_api_error_403() {
        let err = map_api_error(403, "forbidden");
        assert!(matches!(err, crate::ProviderError::Forbidden(_)));
    }

    #[test]
    fn map_api_error_429() {
        let err = map_api_error(429, "rate limited");
        assert!(matches!(err, crate::ProviderError::RateLimited { .. }));
    }

    #[test]
    fn map_api_error_500() {
        let err = map_api_error(500, "internal error");
        assert!(matches!(err, crate::ProviderError::Unavailable(_)));
    }

    #[test]
    fn map_api_error_unknown_falls_back_to_api_error() {
        let err = map_api_error(418, "teapot");
        assert!(matches!(err, crate::ProviderError::ApiError { status: 418, .. }));
    }

    #[test]
    fn map_api_error_422() {
        let err = map_api_error(422, "bad input");
        assert!(matches!(err, crate::ProviderError::InvalidRequest(_)));
    }

    #[test]
    fn map_api_error_unstructured_body() {
        let err = map_api_error(401, "plain text error");
        if let crate::ProviderError::Unauthorized(msg) = err {
            assert_eq!(msg, "plain text error");
        } else {
            panic!("expected Unauthorized");
        }
    }

    #[test]
    fn wire_chat_request_serializes_stream_flag() {
        let req = WireChatRequest {
            model: "gpt-4o".to_string(),
            messages: vec![WireChatMessage {
                role: "user".to_string(),
                content: serde_json::Value::String("hello".to_string()),
                tool_calls: None,
                tool_call_id: None,
            }],
            max_tokens: Some(100),
            temperature: None,
            stream: true,
            tools: vec![],
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""stream":true"#));
        assert!(!json.contains("tools")); // empty vec skipped
    }

    #[test]
    fn wire_chat_message_skips_none_fields() {
        let msg = WireChatMessage {
            role: "user".to_string(),
            content: serde_json::Value::String("hi".to_string()),
            tool_calls: None,
            tool_call_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("tool_calls"));
        assert!(!json.contains("tool_call_id"));
    }
}

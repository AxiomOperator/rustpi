//! Centralized secrets redaction layer.
//!
//! [`Redactor`] scans strings for common secret patterns (bearer tokens, API keys,
//! key=value credential pairs, etc.) and replaces the secret values with `[REDACTED]`,
//! preserving the key/prefix for diagnostic context.

use regex::Regex;
use serde_json::Value;

/// A compiled set of secret-detection patterns.
pub struct Redactor {
    patterns: Vec<(Regex, &'static str)>,
}

impl Redactor {
    /// Build a `Redactor` with the default set of secret patterns.
    pub fn new() -> Self {
        // Each tuple is (pattern, replacement).
        // Capture group 1 preserves the non-secret prefix; the rest is replaced.
        let raw: &[(&str, &str)] = &[
            // Bearer tokens: preserve "Bearer " prefix
            (
                r"(?i)(Bearer\s+)[A-Za-z0-9\-._~+/]+=*",
                "${1}[REDACTED]",
            ),
            // Common API key prefixes (sk-, pk-, gsk_, ghp_, gho_, ghu_, ghs_, xoxb-, xoxp-, AKIA)
            (
                r"(sk-|pk-|gsk_|ghp_|gho_|ghu_|ghs_|xoxb-|xoxp-|AKIA)[A-Za-z0-9]{8,}",
                "${1}[REDACTED]",
            ),
            // Authorization / X-Api-Key / X-Auth-Token headers: preserve "Header: " prefix
            (
                r"(?i)((?:Authorization|X-Api-Key|X-Auth-Token):\s*)\S+",
                "${1}[REDACTED]",
            ),
            // Generic key=value / key:value credential fields
            (
                r"(?i)((?:token|secret|password|api_key|apikey|access_token|refresh_token)\s*[=:]\s*)\S+",
                "${1}[REDACTED]",
            ),
            // Base64-looking long value after a token keyword
            (
                r"(?i)(token[^\s]*\s+)[A-Za-z0-9+/]{32,}={0,2}",
                "${1}[REDACTED]",
            ),
        ];

        let patterns = raw
            .iter()
            .filter_map(|(pat, repl)| Regex::new(pat).ok().map(|re| (re, *repl)))
            .collect();

        Self { patterns }
    }

    /// Redact secrets from `text`, replacing matched values with `[REDACTED]`.
    pub fn redact(&self, text: &str) -> String {
        let mut result = text.to_owned();
        for (re, replacement) in &self.patterns {
            let replaced = re.replace_all(&result, *replacement).into_owned();
            result = replaced;
        }
        result
    }

    /// Recursively redact secrets in all string fields of a JSON [`Value`].
    /// When processing an Object, string values whose key is a known secret
    /// field name are fully redacted regardless of value content.
    pub fn redact_json(&self, value: &Value) -> Value {
        match value {
            Value::String(s) => Value::String(self.redact(s)),
            Value::Array(arr) => Value::Array(arr.iter().map(|v| self.redact_json(v)).collect()),
            Value::Object(map) => {
                const SECRET_KEYS: &[&str] = &[
                    "token", "secret", "password", "api_key", "apikey",
                    "access_token", "refresh_token", "authorization",
                    "x-api-key", "x-auth-token",
                ];
                let redacted = map
                    .iter()
                    .map(|(k, v)| {
                        let key_lower = k.to_lowercase();
                        let is_secret_key = SECRET_KEYS.iter().any(|sk| key_lower.contains(sk));
                        let new_val = if is_secret_key {
                            match v {
                                Value::String(_) => Value::String("[REDACTED]".to_owned()),
                                _ => self.redact_json(v),
                            }
                        } else {
                            self.redact_json(v)
                        };
                        (k.clone(), new_val)
                    })
                    .collect();
                Value::Object(redacted)
            }
            other => other.clone(),
        }
    }

    /// Returns `true` if any pattern matches `text`.
    pub fn contains_secret(&self, text: &str) -> bool {
        self.patterns.iter().any(|(re, _)| re.is_match(text))
    }
}

impl Default for Redactor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bearer_token_is_redacted() {
        let r = Redactor::new();
        let out = r.redact("Authorization: Bearer abc123xyz");
        assert!(out.contains("[REDACTED]"), "got: {out}");
        assert!(!out.contains("abc123xyz"), "secret still present: {out}");
    }

    #[test]
    fn api_key_sk_prefix_redacted() {
        let r = Redactor::new();
        let out = r.redact("sk-abcdefghijklmnop");
        assert!(out.contains("[REDACTED]"), "got: {out}");
        assert!(out.contains("sk-"), "prefix should be preserved: {out}");
    }

    #[test]
    fn api_key_ghp_prefix_redacted() {
        let r = Redactor::new();
        let out = r.redact("ghp_abcdefghijklmnop");
        assert!(out.contains("[REDACTED]"), "got: {out}");
        assert!(out.contains("ghp_"), "prefix should be preserved: {out}");
    }

    #[test]
    fn token_equals_value_redacted() {
        let r = Redactor::new();
        let out = r.redact("access_token=secret123");
        assert!(out.contains("[REDACTED]"), "got: {out}");
        assert!(out.contains("access_token="), "key should be preserved: {out}");
    }

    #[test]
    fn plain_text_unchanged() {
        let r = Redactor::new();
        let input = "hello world foo bar";
        assert_eq!(r.redact(input), input);
    }

    #[test]
    fn empty_string_unchanged() {
        let r = Redactor::new();
        assert_eq!(r.redact(""), "");
    }

    #[test]
    fn redact_json_string_field() {
        let r = Redactor::new();
        let input = json!({ "token": "secret-value", "name": "alice" });
        let out = r.redact_json(&input);
        let token_val = out["token"].as_str().unwrap();
        assert!(token_val.contains("[REDACTED]"), "got: {token_val}");
        // Non-secret fields unchanged
        assert_eq!(out["name"].as_str().unwrap(), "alice");
    }

    #[test]
    fn redact_json_nested() {
        let r = Redactor::new();
        let input = json!({
            "headers": {
                "Authorization": "Bearer supersecrettoken123"
            }
        });
        let out = r.redact_json(&input);
        let auth = out["headers"]["Authorization"].as_str().unwrap();
        assert!(auth.contains("[REDACTED]"), "got: {auth}");
    }

    #[test]
    fn contains_secret_true() {
        let r = Redactor::new();
        assert!(r.contains_secret("Authorization: Bearer mytoken123abc"));
    }

    #[test]
    fn contains_secret_false() {
        let r = Redactor::new();
        assert!(!r.contains_secret("hello world, nothing sensitive here"));
    }

    #[test]
    fn redact_multiple_secrets_in_one_string() {
        let r = Redactor::new();
        let input = "Bearer abc123defxyz and sk-secretkey123456";
        let out = r.redact(input);
        assert!(out.contains("[REDACTED]"), "expected redaction, got: {out}");
        assert!(!out.contains("abc123defxyz"), "bearer secret still present: {out}");
        assert!(!out.contains("secretkey123456"), "api key secret still present: {out}");
    }

    #[test]
    fn redact_json_preserves_non_secret_fields() {
        let r = Redactor::new();
        let input = json!({
            "api_key": "sk-supersecret12345678",
            "username": "alice",
            "email": "alice@example.com",
            "count": 42
        });
        let out = r.redact_json(&input);
        assert_eq!(out["username"].as_str().unwrap(), "alice");
        assert_eq!(out["email"].as_str().unwrap(), "alice@example.com");
        assert_eq!(out["count"].as_i64().unwrap(), 42);
        let api_key_val = out["api_key"].as_str().unwrap();
        assert!(api_key_val.contains("[REDACTED]"), "api_key must be redacted, got: {api_key_val}");
    }

    #[test]
    fn redact_does_not_leak_partial_secret() {
        let r = Redactor::new();
        // Pattern requires 8+ chars after prefix; "sk-abc" has only 3 → must NOT match
        let short = "sk-abc";
        let out = r.redact(short);
        assert_eq!(out, short, "short string below minimum length must not be redacted, got: {out}");
    }

    #[test]
    fn redact_ghp_api_key() {
        let r = Redactor::new();
        let out = r.redact("ghp_Abc123DefGhi456Jkl");
        assert!(out.contains("[REDACTED]"), "ghp_ key must be redacted, got: {out}");
        assert!(!out.contains("Abc123DefGhi456Jkl"), "secret portion still present: {out}");
        assert!(out.contains("ghp_"), "prefix should be preserved: {out}");
    }
}

//! Token estimation utilities.
//!
//! Uses a ~4 characters-per-token heuristic (GPT-family approximation).
//! Real tokenization is deferred until a provider-specific tokenizer is
//! integrated. This heuristic is intentionally conservative and consistent
//! with `agent_core::prompt::estimate_tokens`.

/// Estimate token count from text length.
/// ~4 characters per token (GPT-family heuristic).
pub fn estimate(text: &str) -> u32 {
    ((text.len() as f32) / 4.0).ceil() as u32
}

/// Estimate token count from byte count (assumes ~1 byte/char for ASCII-heavy text).
pub fn estimate_bytes(byte_count: u64) -> u32 {
    ((byte_count as f32) / 4.0).ceil() as u32
}

/// Token budget tracking.
#[derive(Debug, Clone)]
pub struct Budget {
    pub total: u32,
    pub used: u32,
}

impl Budget {
    pub fn new(total: u32) -> Self {
        Self { total, used: 0 }
    }

    pub fn remaining(&self) -> u32 {
        self.total.saturating_sub(self.used)
    }

    pub fn consume(&mut self, tokens: u32) -> bool {
        if self.used + tokens <= self.total {
            self.used += tokens;
            true
        } else {
            false
        }
    }

    pub fn would_fit(&self, tokens: u32) -> bool {
        self.used + tokens <= self.total
    }

    pub fn is_exhausted(&self) -> bool {
        self.used >= self.total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_empty() {
        assert_eq!(estimate(""), 0);
    }

    #[test]
    fn estimate_short_text() {
        // "hello" = 5 chars → ceil(5/4) = 2
        assert_eq!(estimate("hello"), 2);
    }

    #[test]
    fn estimate_four_chars() {
        assert_eq!(estimate("test"), 1);
    }

    #[test]
    fn budget_consume_fits() {
        let mut b = Budget::new(100);
        assert!(b.consume(50));
        assert_eq!(b.used, 50);
        assert_eq!(b.remaining(), 50);
    }

    #[test]
    fn budget_consume_overflow() {
        let mut b = Budget::new(10);
        assert!(!b.consume(11));
        assert_eq!(b.used, 0); // not consumed
    }

    #[test]
    fn budget_exhausted() {
        let mut b = Budget::new(5);
        b.consume(5);
        assert!(b.is_exhausted());
    }
}

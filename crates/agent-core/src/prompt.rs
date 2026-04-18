//! Prompt assembly pipeline.
//!
//! [`PromptAssembler`] accepts structured input blocks and produces an
//! [`AssembledPrompt`] with ordered sections. Token accounting is placeholder-only
//! in Phase 1; real tokenization is deferred to Phase 6 when the context engine
//! is built.
//!
//! # Section ordering
//! Sections are assembled in this order (omitted if absent):
//! 1. System / personality
//! 2. Memory blocks (long-term context)
//! 3. Context blocks (file/working-set content)
//! 4. Conversation history
//! 5. User input

use crate::{
    error::AgentError,
    types::{AgentEvent, RunId},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

/// A token budget hint passed to the assembler.
///
/// Phase 1 placeholder — actual token counting requires provider-specific
/// tokenizers and is deferred to Phase 6.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudget {
    /// Total token budget for the assembled prompt (context window limit).
    pub total: u32,
    /// Tokens reserved for the model's response.
    pub reserved_for_response: u32,
}

impl TokenBudget {
    pub fn new(total: u32, reserved_for_response: u32) -> Self {
        Self { total, reserved_for_response }
    }

    /// Tokens available for prompt content.
    pub fn available(&self) -> u32 {
        self.total.saturating_sub(self.reserved_for_response)
    }
}

impl Default for TokenBudget {
    fn default() -> Self {
        // Sensible placeholder defaults; real values come from provider capabilities.
        Self::new(128_000, 4_096)
    }
}

/// A single section within an assembled prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSection {
    pub kind: SectionKind,
    pub content: String,
    /// Placeholder estimated token count (len/4 heuristic).
    pub estimated_tokens: u32,
}

impl PromptSection {
    pub fn new(kind: SectionKind, content: impl Into<String>) -> Self {
        let content = content.into();
        let estimated_tokens = estimate_tokens(&content);
        Self { kind, content, estimated_tokens }
    }
}

/// The logical role of a prompt section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    /// Agent personality, identity, and standing instructions.
    System,
    /// Long-term memory retrieved from the vault or vector store.
    Memory,
    /// Working-set file content from the context engine.
    Context,
    /// Prior conversation turns (assistant + user messages).
    History,
    /// The current user input or task description.
    UserInput,
}

/// The result of assembling a prompt from structured inputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssembledPrompt {
    /// The run this prompt belongs to.
    pub run_id: RunId,
    /// Ordered prompt sections, in assembly order.
    pub sections: Vec<PromptSection>,
    /// Sum of `estimated_tokens` across all sections.
    pub total_estimated_tokens: u32,
    /// The budget used for assembly.
    pub budget: TokenBudget,
    /// Whether any sections were dropped due to budget pressure.
    /// Phase 1: always false — real truncation in Phase 6.
    pub truncated: bool,
}

impl AssembledPrompt {
    /// Render the assembled prompt as a single string, sections separated by
    /// double newlines. Suitable for providers that take a raw string input.
    pub fn render(&self) -> String {
        self.sections
            .iter()
            .map(|s| s.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Return all sections of a given kind.
    pub fn sections_of_kind(&self, kind: &SectionKind) -> Vec<&PromptSection> {
        self.sections.iter().filter(|s| &s.kind == kind).collect()
    }
}

/// Builder for constructing a prompt from structured inputs.
///
/// # Example
/// ```rust
/// use agent_core::prompt::{PromptAssembler, TokenBudget};
/// use agent_core::types::RunId;
///
/// let run_id = RunId::new();
/// let prompt = PromptAssembler::new(run_id)
///     .system("You are a helpful coding assistant.")
///     .user_input("Explain how async/await works in Rust.")
///     .assemble()
///     .unwrap();
///
/// assert!(!prompt.0.sections.is_empty());
/// ```
pub struct PromptAssembler {
    run_id: RunId,
    system: Option<String>,
    memory_blocks: Vec<String>,
    context_blocks: Vec<String>,
    history: Vec<String>,
    user_input: Option<String>,
    budget: TokenBudget,
}

impl PromptAssembler {
    pub fn new(run_id: RunId) -> Self {
        Self {
            run_id,
            system: None,
            memory_blocks: Vec::new(),
            context_blocks: Vec::new(),
            history: Vec::new(),
            user_input: None,
            budget: TokenBudget::default(),
        }
    }

    /// Set the system / personality block.
    pub fn system(mut self, content: impl Into<String>) -> Self {
        self.system = Some(content.into());
        self
    }

    /// Add a memory block (e.g. vault document or retrieved memory).
    pub fn memory(mut self, content: impl Into<String>) -> Self {
        self.memory_blocks.push(content.into());
        self
    }

    /// Add a context block (e.g. file content from the working set).
    pub fn context(mut self, content: impl Into<String>) -> Self {
        self.context_blocks.push(content.into());
        self
    }

    /// Add a conversation history entry.
    pub fn history_entry(mut self, content: impl Into<String>) -> Self {
        self.history.push(content.into());
        self
    }

    /// Set the user's input for this run.
    pub fn user_input(mut self, content: impl Into<String>) -> Self {
        self.user_input = Some(content.into());
        self
    }

    /// Override the default token budget.
    pub fn with_budget(mut self, budget: TokenBudget) -> Self {
        self.budget = budget;
        self
    }

    /// Assemble the prompt into an [`AssembledPrompt`].
    ///
    /// Returns `Err` if no user input is provided (the prompt would be vacuous).
    pub fn assemble(self) -> Result<(AssembledPrompt, AgentEvent), AgentError> {
        if self.user_input.is_none() && self.system.is_none() {
            return Err(AgentError::PromptAssembly(
                "prompt must have at least a system or user_input block".to_string(),
            ));
        }

        let mut sections = Vec::new();

        if let Some(sys) = self.system {
            sections.push(PromptSection::new(SectionKind::System, sys));
        }
        for mem in self.memory_blocks {
            sections.push(PromptSection::new(SectionKind::Memory, mem));
        }
        for ctx in self.context_blocks {
            sections.push(PromptSection::new(SectionKind::Context, ctx));
        }
        for hist in self.history {
            sections.push(PromptSection::new(SectionKind::History, hist));
        }
        if let Some(input) = self.user_input {
            sections.push(PromptSection::new(SectionKind::UserInput, input));
        }

        let total_estimated_tokens: u32 = sections.iter().map(|s| s.estimated_tokens).sum();
        let section_count = sections.len();

        let prompt = AssembledPrompt {
            run_id: self.run_id.clone(),
            sections,
            total_estimated_tokens,
            budget: self.budget,
            truncated: false,
        };

        let event = AgentEvent::PromptAssembled {
            run_id: self.run_id,
            section_count,
            estimated_tokens: total_estimated_tokens,
            timestamp: Utc::now(),
        };

        Ok((prompt, event))
    }
}

/// Rough token estimate: ~4 characters per token (GPT-family heuristic).
/// Phase 1 placeholder only.
fn estimate_tokens(text: &str) -> u32 {
    ((text.len() as f32) / 4.0).ceil() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RunId;

    #[test]
    fn assemble_minimal_prompt() {
        let run_id = RunId::new();
        let (prompt, event) = PromptAssembler::new(run_id.clone())
            .system("You are an assistant.")
            .user_input("Hello!")
            .assemble()
            .unwrap();

        assert_eq!(prompt.sections.len(), 2);
        assert_eq!(prompt.sections[0].kind, SectionKind::System);
        assert_eq!(prompt.sections[1].kind, SectionKind::UserInput);
        assert!(prompt.total_estimated_tokens > 0);
        assert!(!prompt.truncated);
        assert!(matches!(event, AgentEvent::PromptAssembled { .. }));
    }

    #[test]
    fn section_ordering_is_correct() {
        let run_id = RunId::new();
        let (prompt, _) = PromptAssembler::new(run_id)
            .system("sys")
            .memory("mem")
            .context("ctx")
            .history_entry("hist")
            .user_input("input")
            .assemble()
            .unwrap();

        let kinds: Vec<_> = prompt.sections.iter().map(|s| &s.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &SectionKind::System,
                &SectionKind::Memory,
                &SectionKind::Context,
                &SectionKind::History,
                &SectionKind::UserInput,
            ]
        );
    }

    #[test]
    fn empty_prompt_returns_error() {
        let run_id = RunId::new();
        let result = PromptAssembler::new(run_id).assemble();
        assert!(result.is_err());
    }

    #[test]
    fn render_joins_sections() {
        let run_id = RunId::new();
        let (prompt, _) = PromptAssembler::new(run_id)
            .system("system content")
            .user_input("user content")
            .assemble()
            .unwrap();
        let rendered = prompt.render();
        assert!(rendered.contains("system content"));
        assert!(rendered.contains("user content"));
    }

    #[test]
    fn sections_of_kind() {
        let run_id = RunId::new();
        let (prompt, _) = PromptAssembler::new(run_id)
            .memory("mem1")
            .memory("mem2")
            .user_input("input")
            .assemble()
            .unwrap();
        assert_eq!(prompt.sections_of_kind(&SectionKind::Memory).len(), 2);
        assert_eq!(prompt.sections_of_kind(&SectionKind::UserInput).len(), 1);
    }

    #[test]
    fn token_budget_available() {
        let budget = TokenBudget::new(8192, 1024);
        assert_eq!(budget.available(), 7168);
    }

    #[test]
    fn token_budget_saturation() {
        let budget = TokenBudget::new(100, 200);
        assert_eq!(budget.available(), 0); // saturating_sub
    }

    #[test]
    fn prompt_assembled_event_carries_correct_counts() {
        let run_id = RunId::new();
        let (prompt, event) = PromptAssembler::new(run_id.clone())
            .system("sys")
            .user_input("input")
            .assemble()
            .unwrap();
        match event {
            AgentEvent::PromptAssembled {
                run_id: eid,
                section_count,
                estimated_tokens,
                ..
            } => {
                assert_eq!(eid, run_id);
                assert_eq!(section_count, prompt.sections.len());
                assert_eq!(estimated_tokens, prompt.total_estimated_tokens);
            }
            _ => panic!("expected PromptAssembled"),
        }
    }
}

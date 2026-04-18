//! Personality loader — assembles canonical vault docs into a prompt-ready context.

use context_engine::tokens;

use crate::docs::CanonicalDoc;
use crate::error::MemorySyncError;
use crate::vault::VaultAccessor;

/// Token budget and inclusion flags for personality assembly.
pub struct PersonalityConfig {
    /// Combined token budget for all personality sections.
    pub max_tokens: u32,
    /// Whether to include HEARTBEAT.md in the prompt context.
    pub include_heartbeat: bool,
    /// Whether to include TOOLS.md in the prompt context.
    pub include_tools: bool,
}

impl Default for PersonalityConfig {
    fn default() -> Self {
        Self { max_tokens: 4_000, include_heartbeat: true, include_tools: true }
    }
}

/// A single personality section ready for prompt injection.
pub struct PersonalitySection {
    pub source_doc: CanonicalDoc,
    pub content: String,
    pub tokens: u32,
}

/// Assembled personality context, ready to inject into a `PromptAssembler`.
pub struct PersonalityContext {
    /// Sections in priority order (highest first: Soul → Identity → …).
    pub sections: Vec<PersonalitySection>,
    /// Sum of estimated tokens across all included sections.
    pub estimated_tokens: u32,
    /// Docs successfully loaded.
    pub loaded_docs: Vec<CanonicalDoc>,
    /// Docs that were absent or skipped due to budget.
    pub missing_docs: Vec<CanonicalDoc>,
}

/// Load the personality from the vault and assemble a budget-bounded context.
///
/// Priority order: Soul → Identity → Agents → User → Boot (then Heartbeat and
/// Tools if their respective flags are set in `config`).  Missing docs are
/// silently skipped.  Docs exceeding the per-doc token allowance are
/// truncated with `[truncated]`.
pub fn load_personality(
    accessor: &VaultAccessor,
    config: &PersonalityConfig,
) -> Result<PersonalityContext, MemorySyncError> {
    // Build the ordered list of docs to load.
    let mut ordered: Vec<CanonicalDoc> = CanonicalDoc::all()
        .iter()
        .filter(|d| {
            let is_base = d.included_in_prompt();
            let is_hb = **d == CanonicalDoc::Heartbeat && config.include_heartbeat;
            let is_tools = **d == CanonicalDoc::Tools && config.include_tools;
            is_base || is_hb || is_tools
        })
        .copied()
        .collect();
    // Sort by priority (already sorted since CanonicalDoc::all() is priority-ordered,
    // but be explicit for clarity).
    ordered.sort_by_key(|d| d.prompt_priority());

    let num_docs = ordered.len().max(1) as u32;
    let per_doc_budget = config.max_tokens / num_docs;

    let mut sections = Vec::new();
    let mut loaded_docs = Vec::new();
    let mut missing_docs = Vec::new();
    let mut total_tokens = 0u32;

    for doc in &ordered {
        if total_tokens >= config.max_tokens {
            missing_docs.push(*doc);
            continue;
        }

        match accessor.read_doc(*doc)? {
            None => {
                missing_docs.push(*doc);
            }
            Some(vault_doc) => {
                let content = vault_doc.raw.clone();
                let raw_tokens = tokens::estimate(&content);

                let (final_content, final_tokens) = if raw_tokens > per_doc_budget {
                    let max_chars = (per_doc_budget as usize).saturating_mul(4);
                    let truncated = if max_chars < content.len() {
                        format!("{}[truncated]", &content[..max_chars])
                    } else {
                        content.clone()
                    };
                    let t = tokens::estimate(&truncated);
                    (truncated, t)
                } else {
                    (content, raw_tokens)
                };

                if total_tokens + final_tokens > config.max_tokens {
                    missing_docs.push(*doc);
                    continue;
                }

                total_tokens += final_tokens;
                loaded_docs.push(*doc);
                sections.push(PersonalitySection {
                    source_doc: *doc,
                    content: final_content,
                    tokens: final_tokens,
                });
            }
        }
    }

    Ok(PersonalityContext {
        sections,
        estimated_tokens: total_tokens,
        loaded_docs,
        missing_docs,
    })
}

/// Inject a loaded `PersonalityContext` into a `PromptAssembler` as a combined
/// System section.  Sections are separated by `\n\n---\n\n`.
pub fn inject_personality(
    personality: &PersonalityContext,
    assembler: agent_core::prompt::PromptAssembler,
) -> agent_core::prompt::PromptAssembler {
    if personality.sections.is_empty() {
        return assembler;
    }
    let combined = personality
        .sections
        .iter()
        .map(|s| s.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");
    assembler.system(combined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::prompt::SectionKind;
    use agent_core::types::RunId;

    fn write_doc(dir: &tempfile::TempDir, filename: &str, content: &str) {
        std::fs::write(dir.path().join(filename), content).unwrap();
    }

    #[test]
    fn empty_vault_all_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        let vault = VaultAccessor::open(dir.path()).unwrap();
        let ctx = load_personality(&vault, &PersonalityConfig::default()).unwrap();
        assert!(ctx.sections.is_empty());
        assert!(ctx.loaded_docs.is_empty());
        assert!(!ctx.missing_docs.is_empty());
    }

    #[test]
    fn soul_and_identity_loaded_in_priority_order() {
        let dir = tempfile::TempDir::new().unwrap();
        write_doc(&dir, "SOUL.md", "# Soul\n\nCore ethics.\n");
        write_doc(&dir, "IDENTITY.md", "# Identity\n\nRole info.\n");

        let vault = VaultAccessor::open(dir.path()).unwrap();
        let cfg = PersonalityConfig {
            include_heartbeat: false,
            include_tools: false,
            ..Default::default()
        };
        let ctx = load_personality(&vault, &cfg).unwrap();

        // Soul must come before Identity.
        assert_eq!(ctx.sections[0].source_doc, CanonicalDoc::Soul);
        assert_eq!(ctx.sections[1].source_doc, CanonicalDoc::Identity);
    }

    #[test]
    fn token_budget_truncates_large_doc() {
        let dir = tempfile::TempDir::new().unwrap();
        // ~1000 tokens = 4000 chars; per-doc budget with 1 doc = max_tokens.
        let big = format!("# Soul\n\n{}\n", "x".repeat(8_000));
        write_doc(&dir, "SOUL.md", &big);

        let vault = VaultAccessor::open(dir.path()).unwrap();
        let cfg = PersonalityConfig {
            max_tokens: 100,
            include_heartbeat: false,
            include_tools: false,
        };
        let ctx = load_personality(&vault, &cfg).unwrap();
        assert!(ctx.estimated_tokens <= 100 + 5); // +5 for "[truncated]" rounding
        if !ctx.sections.is_empty() {
            assert!(ctx.sections[0].content.ends_with("[truncated]"));
        }
    }

    #[test]
    fn inject_personality_adds_system_section() {
        let personality = PersonalityContext {
            sections: vec![PersonalitySection {
                source_doc: CanonicalDoc::Soul,
                content: "Core ethics content.".to_string(),
                tokens: 5,
            }],
            estimated_tokens: 5,
            loaded_docs: vec![CanonicalDoc::Soul],
            missing_docs: vec![],
        };

        let run_id = RunId::new();
        let assembler = agent_core::prompt::PromptAssembler::new(run_id);
        let assembler = inject_personality(&personality, assembler);
        let (prompt, _) = assembler.user_input("hello").assemble().unwrap();

        let sys = prompt.sections_of_kind(&SectionKind::System);
        assert_eq!(sys.len(), 1);
        assert!(sys[0].content.contains("Core ethics content."));
    }

    #[test]
    fn inject_personality_no_sections_does_not_add_system() {
        let personality = PersonalityContext {
            sections: vec![],
            estimated_tokens: 0,
            loaded_docs: vec![],
            missing_docs: vec![],
        };

        let run_id = RunId::new();
        let assembler = agent_core::prompt::PromptAssembler::new(run_id);
        let assembler = inject_personality(&personality, assembler);
        let (prompt, _) = assembler.user_input("hello").assemble().unwrap();

        let sys = prompt.sections_of_kind(&SectionKind::System);
        assert!(sys.is_empty());
    }
}

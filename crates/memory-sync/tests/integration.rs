//! Phase 8 integration tests for the memory-sync crate.
//!
//! These tests exercise multi-component workflows using a real (temp) vault
//! directory, covering the full parse → write → read → sync → detect cycle.

use memory_sync::{
    inject_personality, load_personality, CanonicalDoc, MemorySyncError, PersonalityConfig,
    SyncEngine, VaultAccessor, VaultDoc,
};

// ─── helpers ─────────────────────────────────────────────────────────────────

fn open(dir: &tempfile::TempDir) -> VaultAccessor {
    VaultAccessor::open(dir.path()).unwrap()
}

fn write_file(dir: &tempfile::TempDir, name: &str, content: &str) {
    std::fs::write(dir.path().join(name), content).unwrap();
}

fn read_file(dir: &tempfile::TempDir, name: &str) -> String {
    std::fs::read_to_string(dir.path().join(name)).unwrap()
}

// ─── 1. Malformed markdown ────────────────────────────────────────────────────

#[test]
fn malformed_frontmatter_no_colon_via_vault_accessor() {
    let dir = tempfile::TempDir::new().unwrap();
    write_file(&dir, "SOUL.md", "---\nbadline\n---\n# Soul\n\nContent.\n");
    let _vault = open(&dir);
    let result = _vault.read_doc(CanonicalDoc::Soul);
    assert!(
        matches!(result, Err(MemorySyncError::MalformedMarkdown(_))),
        "expected MalformedMarkdown"
    );
}

#[test]
fn unclosed_frontmatter_returns_error() {
    let result = VaultDoc::parse("---\nkey: value\n");
    assert!(matches!(result, Err(MemorySyncError::MalformedMarkdown(_))));
}

#[test]
fn empty_document_is_valid() {
    let doc = VaultDoc::parse("").unwrap();
    assert!(doc.sections.is_empty());
    assert!(doc.frontmatter.is_none());
}

#[test]
fn headings_only_produces_sections_with_empty_content() {
    let doc = VaultDoc::parse("# Alpha\n## Beta\n### Gamma\n").unwrap();
    assert_eq!(doc.sections.len(), 3);
    for sec in &doc.sections {
        // Each section body is just the trailing newline separating headings.
        assert!(!sec.content.contains("content"));
    }
}

// ─── 2. Sync conflicts ────────────────────────────────────────────────────────

#[tokio::test]
async fn detect_conflict_after_manual_edit_then_sync_preserves_human_prose() {
    let dir = tempfile::TempDir::new().unwrap();
    let vault = open(&dir);

    // Write a HEARTBEAT.md that has human prose AND a machine-managed section.
    let initial = "# Notes\n\nHuman wrote this note.\n\n# Heartbeat\n<!-- machine-managed -->\nStatus: idle\n";
    write_file(&dir, "HEARTBEAT.md", initial);

    let engine = SyncEngine::new(VaultAccessor::open(dir.path()).unwrap());
    let expected = "Status: idle";

    // Manual edit: change machine content
    write_file(
        &dir,
        "HEARTBEAT.md",
        "# Notes\n\nHuman wrote this note.\n\n# Heartbeat\n<!-- machine-managed -->\nStatus: MANUALLY_CHANGED\n",
    );

    // Conflict should be detected.
    let conflict = engine
        .detect_conflicts(CanonicalDoc::Heartbeat, expected)
        .unwrap();
    assert!(conflict.is_some(), "expected a conflict");
    let rec = conflict.unwrap();
    assert_eq!(rec.doc, "HEARTBEAT.md");

    // Sync overwrites machine section.
    let result = engine.sync_to_vault("running", &[]).await.unwrap();
    assert!(result.errors.is_empty());

    // Human prose must still be present.
    let content = read_file(&dir, "HEARTBEAT.md");
    assert!(
        content.contains("Human wrote this note."),
        "human prose was lost after sync"
    );
    assert!(
        content.contains("Status: running"),
        "machine section was not updated"
    );
}

// ─── 3. Duplicate note handling ───────────────────────────────────────────────

#[tokio::test]
async fn index_vault_is_consistent_across_two_calls() {
    let dir = tempfile::TempDir::new().unwrap();
    let vault = open(&dir);
    vault.init_defaults().unwrap();
    write_file(&dir, "NOTE_A.md", "# Note A\n\nSome content here.\n");
    write_file(&dir, "NOTE_B.md", "# Note B\n\nOverlapping content here.\n");

    let engine = SyncEngine::new(VaultAccessor::open(dir.path()).unwrap());
    let first = engine.index_vault().await.unwrap();
    let second = engine.index_vault().await.unwrap();

    assert_eq!(
        first.len(),
        second.len(),
        "index_vault returned different counts"
    );
    let mut first_names: Vec<_> = first.iter().map(|(n, _)| n.clone()).collect();
    let mut second_names: Vec<_> = second.iter().map(|(n, _)| n.clone()).collect();
    first_names.sort();
    second_names.sort();
    assert_eq!(first_names, second_names, "index contents differ between calls");
}

#[test]
fn idempotent_write_produces_stable_content() {
    let dir = tempfile::TempDir::new().unwrap();
    let vault = open(&dir);

    let doc = VaultDoc::parse(
        "# Heartbeat\n<!-- machine-managed -->\nStatus: idle\n",
    )
    .unwrap();
    vault.write_doc(CanonicalDoc::Heartbeat, &doc).unwrap();
    let first = read_file(&dir, "HEARTBEAT.md");

    vault.write_doc(CanonicalDoc::Heartbeat, &doc).unwrap();
    let second = read_file(&dir, "HEARTBEAT.md");

    assert_eq!(first, second, "idempotent write changed the file");
}

// ─── 4. Prompt assembly from personality docs ─────────────────────────────────

#[test]
fn load_personality_includes_soul_and_identity() {
    let dir = tempfile::TempDir::new().unwrap();
    write_file(&dir, "SOUL.md", "# Soul\n\nCore ethics content.\n");
    write_file(&dir, "IDENTITY.md", "# Identity\n\nRole definition.\n");

    let vault = open(&dir);
    let cfg = PersonalityConfig {
        include_heartbeat: false,
        include_tools: false,
        ..Default::default()
    };
    let ctx = load_personality(&vault, &cfg).unwrap();

    let soul_present = ctx
        .loaded_docs
        .iter()
        .any(|d| *d == CanonicalDoc::Soul);
    let identity_present = ctx
        .loaded_docs
        .iter()
        .any(|d| *d == CanonicalDoc::Identity);
    assert!(soul_present, "Soul should be loaded");
    assert!(identity_present, "Identity should be loaded");
}

#[test]
fn personality_soul_comes_before_identity() {
    let dir = tempfile::TempDir::new().unwrap();
    write_file(&dir, "SOUL.md", "# Soul\n\nEthics.\n");
    write_file(&dir, "IDENTITY.md", "# Identity\n\nRole.\n");

    let vault = open(&dir);
    let cfg = PersonalityConfig {
        include_heartbeat: false,
        include_tools: false,
        ..Default::default()
    };
    let ctx = load_personality(&vault, &cfg).unwrap();

    let soul_pos = ctx.sections.iter().position(|s| s.source_doc == CanonicalDoc::Soul);
    let id_pos = ctx.sections.iter().position(|s| s.source_doc == CanonicalDoc::Identity);
    assert!(
        soul_pos.is_some() && id_pos.is_some(),
        "both sections must be present"
    );
    assert!(soul_pos < id_pos, "Soul must precede Identity in sections");
}

#[test]
fn inject_personality_adds_system_section_with_content() {
    let dir = tempfile::TempDir::new().unwrap();
    write_file(&dir, "SOUL.md", "# Soul\n\nPersonality ethics.\n");

    let vault = open(&dir);
    let cfg = PersonalityConfig {
        include_heartbeat: false,
        include_tools: false,
        ..Default::default()
    };
    let ctx = load_personality(&vault, &cfg).unwrap();
    assert!(!ctx.sections.is_empty(), "expected at least one section");

    let run_id = agent_core::types::RunId::new();
    let assembler = agent_core::prompt::PromptAssembler::new(run_id);
    let assembler = inject_personality(&ctx, assembler);
    let (prompt, _) = assembler.user_input("hello").assemble().unwrap();

    let sys = prompt.sections_of_kind(&agent_core::prompt::SectionKind::System);
    assert_eq!(sys.len(), 1, "expected exactly one System section");
    assert!(
        sys[0].content.contains("Personality ethics"),
        "System section should contain soul content"
    );
}

// ─── 5. Vault path validation ─────────────────────────────────────────────────

#[test]
fn open_nonexistent_path_returns_error() {
    let result = VaultAccessor::open("/nonexistent/rustpi/vault/xyz_does_not_exist");
    assert!(
        matches!(result, Err(MemorySyncError::VaultNotFound(_))),
        "expected VaultNotFound"
    );
}

#[test]
fn open_file_path_instead_of_directory_returns_error() {
    let dir = tempfile::TempDir::new().unwrap();
    let file_path = dir.path().join("notadir.md");
    std::fs::write(&file_path, "content").unwrap();
    let result = VaultAccessor::open(&file_path);
    assert!(
        matches!(result, Err(MemorySyncError::VaultNotFound(_))),
        "expected VaultNotFound when opening a file as vault"
    );
}

#[test]
fn open_valid_directory_succeeds() {
    let dir = tempfile::TempDir::new().unwrap();
    let result = VaultAccessor::open(dir.path());
    assert!(result.is_ok(), "opening a valid directory should succeed");
}

// ─── 6. Write policy enforcement ─────────────────────────────────────────────

#[test]
fn write_to_readonly_soul_returns_readonly_error() {
    let dir = tempfile::TempDir::new().unwrap();
    let vault = open(&dir);
    let doc = VaultDoc::parse("# Soul\n\nContent.\n").unwrap();
    assert!(matches!(
        vault.write_doc(CanonicalDoc::Soul, &doc),
        Err(MemorySyncError::ReadOnly(_))
    ));
}

#[test]
fn write_to_readonly_identity_returns_readonly_error() {
    let dir = tempfile::TempDir::new().unwrap();
    let vault = open(&dir);
    let doc = VaultDoc::parse("# Identity\n\nContent.\n").unwrap();
    assert!(matches!(
        vault.write_doc(CanonicalDoc::Identity, &doc),
        Err(MemorySyncError::ReadOnly(_))
    ));
}

#[test]
fn write_to_approval_required_user_without_approval_errors() {
    let dir = tempfile::TempDir::new().unwrap();
    let vault = open(&dir);
    let doc = VaultDoc::parse("# User\n\nPrefs.\n").unwrap();
    assert!(matches!(
        vault.write_doc(CanonicalDoc::User, &doc),
        Err(MemorySyncError::ApprovalRequired(_))
    ));
}

#[test]
fn write_to_runtime_writable_heartbeat_succeeds() {
    let dir = tempfile::TempDir::new().unwrap();
    let vault = open(&dir);
    let doc =
        VaultDoc::parse("# Heartbeat\n<!-- machine-managed -->\nStatus: ok\n").unwrap();
    vault.write_doc(CanonicalDoc::Heartbeat, &doc).unwrap();
    let content = read_file(&dir, "HEARTBEAT.md");
    assert!(content.contains("Status: ok"));
}

#[test]
fn write_to_runtime_writable_tools_succeeds() {
    let dir = tempfile::TempDir::new().unwrap();
    let vault = open(&dir);
    let doc = VaultDoc::parse("# Tools\n<!-- machine-managed -->\nbash\n").unwrap();
    vault.write_doc(CanonicalDoc::Tools, &doc).unwrap();
    let content = read_file(&dir, "TOOLS.md");
    assert!(content.contains("bash"));
}

// ─── 7. Round-trips ──────────────────────────────────────────────────────────

#[test]
fn soul_template_round_trips() {
    let template = CanonicalDoc::Soul.default_template();
    let doc1 = VaultDoc::parse(template).unwrap();
    let rendered = doc1.render();
    let doc2 = VaultDoc::parse(&rendered).unwrap();

    assert_eq!(doc1.sections.len(), doc2.sections.len());
    for (s1, s2) in doc1.sections.iter().zip(doc2.sections.iter()) {
        assert_eq!(s1.heading, s2.heading);
        assert_eq!(s1.content, s2.content);
    }
}

#[test]
fn frontmatter_round_trip_preserves_key_values() {
    let text = "---\nauthor: Alice\ntitle: Test Doc\n---\n# Section\n\nBody text.\n";
    let doc1 = VaultDoc::parse(text).unwrap();
    let rendered = doc1.render();
    let doc2 = VaultDoc::parse(&rendered).unwrap();

    let fm1 = doc1.frontmatter.as_ref().unwrap();
    let fm2 = doc2.frontmatter.as_ref().unwrap();
    assert_eq!(fm1.get("author"), fm2.get("author"));
    assert_eq!(fm1.get("title"), fm2.get("title"));
}

#[test]
fn update_machine_sections_preserves_human_prose() {
    let dir = tempfile::TempDir::new().unwrap();
    let vault = open(&dir);

    let initial = "# My Notes\n\nHuman wrote this important prose.\n\n# Heartbeat\n<!-- machine-managed -->\nStatus: idle\n";
    write_file(&dir, "HEARTBEAT.md", initial);

    vault
        .update_machine_sections(CanonicalDoc::Heartbeat, &[("Heartbeat", "Status: updated")])
        .unwrap();

    let content = read_file(&dir, "HEARTBEAT.md");
    assert!(
        content.contains("Human wrote this important prose."),
        "human prose was not preserved: {content}"
    );
    assert!(
        content.contains("Status: updated"),
        "machine section was not updated: {content}"
    );
}

// ─── 8. Init defaults ────────────────────────────────────────────────────────

#[test]
fn init_defaults_creates_all_canonical_docs() {
    let dir = tempfile::TempDir::new().unwrap();
    let vault = open(&dir);
    let created = vault.init_defaults().unwrap();
    assert_eq!(created.len(), CanonicalDoc::all().len());

    for &doc in CanonicalDoc::all() {
        assert!(
            dir.path().join(doc.filename()).exists(),
            "{} was not created",
            doc.filename()
        );
    }
}

#[test]
fn init_defaults_is_idempotent_and_does_not_overwrite() {
    let dir = tempfile::TempDir::new().unwrap();
    let vault = open(&dir);

    // First init.
    vault.init_defaults().unwrap();
    // Overwrite one file with custom content to verify it's not wiped.
    write_file(&dir, "SOUL.md", "# Soul\n\nCustom human content.\n");

    // Second init — should not overwrite.
    let created_second = vault.init_defaults().unwrap();
    assert!(
        created_second.is_empty(),
        "second init_defaults should not create any files"
    );
    let soul = read_file(&dir, "SOUL.md");
    assert!(
        soul.contains("Custom human content."),
        "init_defaults overwrote existing SOUL.md"
    );
}

// ─── 9. Token budget in personality ──────────────────────────────────────────

#[test]
fn large_soul_doc_is_truncated_within_budget() {
    let dir = tempfile::TempDir::new().unwrap();
    // ~16 000 chars ≈ 4 000 tokens — well over the 200-token budget.
    let big_content = format!("# Soul\n\n{}\n", "word ".repeat(3_200));
    write_file(&dir, "SOUL.md", &big_content);

    let vault = open(&dir);
    let cfg = PersonalityConfig {
        max_tokens: 200,
        include_heartbeat: false,
        include_tools: false,
    };
    let ctx = load_personality(&vault, &cfg).unwrap();

    // Must not panic, and estimated_tokens must be within budget (plus small rounding).
    assert!(
        ctx.estimated_tokens <= 210,
        "estimated_tokens {} exceeds budget",
        ctx.estimated_tokens
    );
    if !ctx.sections.is_empty() {
        assert!(
            ctx.sections[0].content.ends_with("[truncated]"),
            "large doc should be truncated"
        );
    }
}

#[test]
fn personality_tokens_field_matches_sum_of_sections() {
    let dir = tempfile::TempDir::new().unwrap();
    write_file(&dir, "SOUL.md", "# Soul\n\nEthics.\n");
    write_file(&dir, "IDENTITY.md", "# Identity\n\nRole.\n");

    let vault = open(&dir);
    let ctx = load_personality(&vault, &PersonalityConfig::default()).unwrap();

    let section_sum: u32 = ctx.sections.iter().map(|s| s.tokens).sum();
    assert_eq!(
        ctx.estimated_tokens, section_sum,
        "estimated_tokens should equal sum of section tokens"
    );
}

// ─── 10. Sync to vault ───────────────────────────────────────────────────────

#[tokio::test]
async fn sync_to_vault_writes_status_and_tools() {
    let dir = tempfile::TempDir::new().unwrap();
    {
        let setup = open(&dir);
        setup.init_defaults().unwrap();
    }

    let engine = SyncEngine::new(VaultAccessor::open(dir.path()).unwrap());
    let result = engine
        .sync_to_vault("running", &["tool_a", "tool_b"])
        .await
        .unwrap();

    assert!(result.errors.is_empty(), "sync errors: {:?}", result.errors);

    let hb = read_file(&dir, "HEARTBEAT.md");
    assert!(hb.contains("running"), "HEARTBEAT.md missing 'running'");

    let tools = read_file(&dir, "TOOLS.md");
    assert!(tools.contains("tool_a"), "TOOLS.md missing 'tool_a'");
    assert!(tools.contains("tool_b"), "TOOLS.md missing 'tool_b'");
}

#[tokio::test]
async fn sync_to_vault_on_empty_vault_creates_docs() {
    let dir = tempfile::TempDir::new().unwrap();
    // No init — vault is empty; sync should create the files from templates.
    let engine = SyncEngine::new(VaultAccessor::open(dir.path()).unwrap());
    let result = engine.sync_to_vault("idle", &[]).await.unwrap();

    assert!(result.errors.is_empty(), "sync errors: {:?}", result.errors);
    assert!(dir.path().join("HEARTBEAT.md").exists());
    assert!(dir.path().join("TOOLS.md").exists());
}

/// Comprehensive integration tests for the Phase 6 context engine.
///
/// Tests are grouped by component/concern:
///   1. Ignore correctness
///   2. Token budgeting
///   3. Context truncation
///   4. Summarization / compaction fallback
///   5. Working-set selection stability
///   6. Full pipeline integration
///   7. Edge cases
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use context_engine::{
    compactor::{compact, summarize_rust_source, CompactionStrategy},
    ignore::IgnoreEngine,
    memory::{MemoryQuery, MemoryRetriever, NoopMemory, StaticMemory, VaultMemory},
    packer::{ContextPacker, MemorySnippet, PackerConfig},
    relevance::{score, score_all, RelevanceHints, ScoredEntry},
    scanner::{FileEntry, Scanner, ScannerConfig},
    tokens::{self, Budget},
    workset::{select, SelectionReason, SelectedFile, WorksetConfig},
    ContextEngine, ContextError, EngineConfig, PackedContext, RelevanceHints as PublicHints,
};
use tempfile::TempDir;

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

fn make_selected(path: &str, score: f32, tokens: u32) -> SelectedFile {
    SelectedFile {
        path: PathBuf::from(path),
        size_bytes: (tokens as u64) * 4,
        score,
        estimated_tokens: tokens,
        reason: SelectionReason::TopRelevance,
    }
}

fn make_file_entry(path: &str, size: u64, ext: Option<&str>) -> FileEntry {
    FileEntry {
        path: PathBuf::from(path),
        size_bytes: size,
        extension: ext.map(String::from),
        is_text: true,
    }
}

fn make_memory_snippet(source: &str, content: &str) -> MemorySnippet {
    MemorySnippet {
        source: source.to_string(),
        content: content.to_string(),
        tokens: tokens::estimate(content),
    }
}

// ────────────────────────────────────────────────────────────────────────────
// 1. Ignore correctness
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn ignore_gitignore_excludes_matched_files() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".gitignore"), "target/\n*.lock\n").unwrap();
    fs::create_dir_all(dir.path().join("target/debug")).unwrap();
    fs::write(dir.path().join("target/debug/app"), "binary").unwrap();
    fs::write(dir.path().join("Cargo.lock"), "lock").unwrap();
    fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

    let engine = IgnoreEngine::new(dir.path());

    assert!(engine.is_ignored(&dir.path().join("target"), true), "target/ should be ignored");
    assert!(engine.is_ignored(&dir.path().join("Cargo.lock"), false), "*.lock should be ignored");
    assert!(!engine.is_ignored(&dir.path().join("main.rs"), false), "main.rs should not be ignored");
}

#[test]
fn ignore_contextignore_works_independently() {
    let dir = TempDir::new().unwrap();
    // No .gitignore — only .contextignore
    fs::write(dir.path().join(".contextignore"), "docs/\nbuild/\n").unwrap();
    fs::create_dir_all(dir.path().join("docs")).unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();

    let engine = IgnoreEngine::new(dir.path());

    assert!(engine.is_ignored(&dir.path().join("docs"), true), "docs/ should be ignored via .contextignore");
    assert!(engine.is_ignored(&dir.path().join("build"), true), "build/ should be ignored via .contextignore");
    assert!(!engine.is_ignored(&dir.path().join("src"), true), "src/ should not be ignored");
}

#[test]
fn ignore_non_ignored_files_appear_in_scan() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".gitignore"), "secret.txt\n").unwrap();
        fs::write(dir.path().join("secret.txt"), "top secret").unwrap();
        fs::write(dir.path().join("public.rs"), "pub fn foo() {}").unwrap();

        let scanner = Scanner::new(dir.path());
        let (entries, _stats) = scanner.scan().await.unwrap();

        let paths: Vec<_> = entries.iter().map(|e| e.path.file_name().unwrap().to_str().unwrap().to_owned()).collect();
        assert!(!paths.contains(&"secret.txt".to_string()), "secret.txt should be filtered");
        assert!(paths.contains(&"public.rs".to_string()), "public.rs should appear");
    });
}

#[test]
fn ignore_hidden_files_always_ignored() {
    let dir = TempDir::new().unwrap();
    let engine = IgnoreEngine::new(dir.path());

    assert!(engine.is_ignored(&dir.path().join(".hidden"), false));
    assert!(engine.is_ignored(&dir.path().join(".env"), false));
    // .env.example is explicitly allowed
    assert!(!engine.is_ignored(&dir.path().join(".env.example"), false));
}

#[test]
fn ignore_binary_extensions_detected() {
    assert!(IgnoreEngine::is_binary_extension("png"));
    assert!(IgnoreEngine::is_binary_extension("exe"));
    assert!(IgnoreEngine::is_binary_extension("wasm"));
    assert!(!IgnoreEngine::is_binary_extension("rs"));
    assert!(!IgnoreEngine::is_binary_extension("md"));
    assert!(!IgnoreEngine::is_binary_extension("toml"));
}

// ────────────────────────────────────────────────────────────────────────────
// 2. Token budgeting
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn token_estimate_non_zero_for_non_empty() {
    let result = tokens::estimate("hello world");
    assert!(result > 0, "non-empty string must yield > 0 tokens");
}

#[test]
fn token_estimate_zero_for_empty() {
    assert_eq!(tokens::estimate(""), 0);
}

#[test]
fn token_estimate_four_chars_per_token() {
    // Exact multiple: 4 chars → 1 token
    assert_eq!(tokens::estimate("abcd"), 1);
    // 8 chars → 2 tokens
    assert_eq!(tokens::estimate("abcdefgh"), 2);
    // 5 chars → ceil(5/4)=2 tokens
    assert_eq!(tokens::estimate("abcde"), 2);
}

#[test]
fn token_estimate_bytes_plausible() {
    // 400 bytes of ASCII ≈ 100 tokens
    assert_eq!(tokens::estimate_bytes(400), 100);
    // 1 byte → 1 token (ceil)
    assert_eq!(tokens::estimate_bytes(1), 1);
}

#[test]
fn budget_remaining_decreases_on_consume() {
    let mut b = Budget::new(100);
    assert_eq!(b.remaining(), 100);
    b.consume(30);
    assert_eq!(b.remaining(), 70);
    b.consume(70);
    assert_eq!(b.remaining(), 0);
}

#[test]
fn budget_overflow_rejected_and_unchanged() {
    let mut b = Budget::new(50);
    let ok = b.consume(60);
    assert!(!ok, "overflow should return false");
    assert_eq!(b.used, 0, "used should remain 0 after failed consume");
    assert_eq!(b.remaining(), 50);
}

#[test]
fn budget_saturating_remaining() {
    // remaining() uses saturating_sub, so can never underflow
    let b = Budget { total: 10, used: 10 };
    assert_eq!(b.remaining(), 0);
}

#[test]
fn budget_is_exhausted_when_full() {
    let mut b = Budget::new(5);
    assert!(!b.is_exhausted());
    b.consume(5);
    assert!(b.is_exhausted());
}

#[test]
fn budget_would_fit_checks_correctly() {
    let mut b = Budget::new(100);
    b.consume(80);
    assert!(b.would_fit(20));
    assert!(!b.would_fit(21));
}

// ────────────────────────────────────────────────────────────────────────────
// 3. Context truncation
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn packer_stays_within_budget_when_content_exceeds_it() {
    let dir = TempDir::new().unwrap();

    // Create a file with ~1600 chars = ~400 tokens
    let big_content = "fn foo() { }\n".repeat(123); // ~1599 chars
    fs::write(dir.path().join("big.rs"), &big_content).unwrap();

    let files = vec![SelectedFile {
        path: dir.path().join("big.rs"),
        size_bytes: big_content.len() as u64,
        score: 0.9,
        estimated_tokens: tokens::estimate(&big_content),
        reason: SelectionReason::TopRelevance,
    }];

    // Budget much smaller than file
    let packer = ContextPacker::new(PackerConfig {
        token_budget: 50,
        memory_budget: 0,
        max_file_read_bytes: 64 * 1024,
    });
    let result = packer.pack(files, vec![]).await.unwrap();
    assert!(result.total_tokens <= 50, "total tokens {} should be <= 50", result.total_tokens);
}

#[tokio::test]
async fn packer_marks_truncated_when_file_is_cut() {
    let dir = TempDir::new().unwrap();
    let content = "line one\nline two\nline three\nline four\n".repeat(50);
    fs::write(dir.path().join("long.rs"), &content).unwrap();

    let files = vec![SelectedFile {
        path: dir.path().join("long.rs"),
        size_bytes: content.len() as u64,
        score: 0.8,
        estimated_tokens: tokens::estimate(&content),
        reason: SelectionReason::TopRelevance,
    }];

    let packer = ContextPacker::new(PackerConfig {
        token_budget: 20,
        memory_budget: 0,
        max_file_read_bytes: 64 * 1024,
    });
    let result = packer.pack(files, vec![]).await.unwrap();
    assert!(result.truncated, "truncated flag should be set");
    if let Some(block) = result.blocks.first() {
        assert!(block.truncated, "block.truncated should be set");
        assert!(
            block.content.contains("[truncated]"),
            "truncated marker should appear in content"
        );
    }
}

#[tokio::test]
async fn packer_high_priority_file_included_before_low() {
    let dir = TempDir::new().unwrap();

    // Two files; budget fits only one
    let content_a = "a".repeat(100); // ~25 tokens
    let content_b = "b".repeat(100); // ~25 tokens
    fs::write(dir.path().join("high.rs"), &content_a).unwrap();
    fs::write(dir.path().join("low.rs"), &content_b).unwrap();

    let files = vec![
        SelectedFile {
            path: dir.path().join("high.rs"),
            size_bytes: 100,
            score: 0.9,
            estimated_tokens: 25,
            reason: SelectionReason::TopRelevance,
        },
        SelectedFile {
            path: dir.path().join("low.rs"),
            size_bytes: 100,
            score: 0.1,
            estimated_tokens: 25,
            reason: SelectionReason::TopRelevance,
        },
    ];

    let packer = ContextPacker::new(PackerConfig {
        token_budget: 30, // barely fits one file
        memory_budget: 0,
        max_file_read_bytes: 64 * 1024,
    });
    let result = packer.pack(files, vec![]).await.unwrap();
    let included: Vec<_> = result.included_files.iter().map(|p| p.file_name().unwrap().to_str().unwrap()).collect();
    assert!(included.contains(&"high.rs"), "high-priority file should be included first");
}

// ────────────────────────────────────────────────────────────────────────────
// 4. Summarization / compaction fallback
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn compact_drop_low_removes_below_threshold() {
    let files = vec![
        make_selected("/p/high.rs", 0.9, 200),
        make_selected("/p/mid.rs", 0.5, 200),
        make_selected("/p/low.rs", 0.1, 200),
    ];
    let (kept, note) = compact(files, 1000, CompactionStrategy::DropLow { threshold: 0.4 });
    assert_eq!(kept.len(), 2, "files below 0.4 threshold should be dropped");
    assert!(note.contains("dropped"), "note should mention dropped files");
}

#[test]
fn compact_drop_low_keeps_all_when_none_below_threshold() {
    let files = vec![
        make_selected("/p/a.rs", 0.8, 100),
        make_selected("/p/b.rs", 0.9, 100),
    ];
    let (kept, _) = compact(files, 1000, CompactionStrategy::DropLow { threshold: 0.5 });
    assert_eq!(kept.len(), 2);
}

#[test]
fn compact_truncate_reduces_token_estimates() {
    let files = vec![
        make_selected("/p/a.rs", 0.7, 1000),
        make_selected("/p/b.rs", 0.6, 800),
    ];
    let (result, note) = compact(files, 500, CompactionStrategy::Truncate { fraction: 0.5 });
    assert_eq!(result[0].estimated_tokens, 500);
    assert_eq!(result[1].estimated_tokens, 400);
    assert!(note.contains("50%"), "note should mention truncation fraction");
}

#[test]
fn compact_extract_declarations_returns_all_files() {
    // ExtractDeclarations just annotates; actual extraction handled by packer
    let files = vec![
        make_selected("/p/a.rs", 0.8, 300),
        make_selected("/p/b.rs", 0.6, 200),
    ];
    let (kept, note) = compact(files, 400, CompactionStrategy::ExtractDeclarations);
    assert_eq!(kept.len(), 2, "ExtractDeclarations preserves all files");
    assert!(note.contains("declarations"), "note should mention declarations");
}

#[test]
fn summarize_rust_source_extracts_declarations() {
    let src = r#"
//! Module doc.

use std::fmt;

pub struct Config {
    pub name: String,
    value: i32,
}

pub fn process(cfg: &Config) -> String {
    cfg.name.clone()
}

pub trait Handler {
    fn handle(&self) -> bool;
}

impl Config {
    pub fn new(name: &str) -> Self {
        Config { name: name.to_string(), value: 0 }
    }
}

enum State {
    Running,
    Stopped,
}
"#;
    let summary = summarize_rust_source(src);

    assert!(summary.contains("pub struct Config"), "should extract struct");
    assert!(summary.contains("pub fn process"), "should extract fn");
    assert!(summary.contains("pub trait Handler"), "should extract trait");
    assert!(summary.contains("impl Config"), "should extract impl");
    assert!(summary.contains("enum State"), "should extract enum");
    // Bodies should NOT appear
    assert!(!summary.contains("cfg.name.clone()"), "fn body should be excluded");
    assert!(!summary.contains("name.to_string()"), "impl body should be excluded");
}

#[test]
fn summarize_rust_source_is_smaller_than_original() {
    let src = "fn big() {\n".to_string()
        + &"    let x = 1;\n".repeat(100)
        + "}\n";
    let summary = summarize_rust_source(&src);
    assert!(
        summary.len() < src.len(),
        "summary ({} bytes) should be smaller than source ({} bytes)",
        summary.len(),
        src.len()
    );
}

// ────────────────────────────────────────────────────────────────────────────
// 5. Working-set selection stability
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn workset_selection_is_deterministic() {
    let entries: Vec<ScoredEntry> = (0..20)
        .map(|i| ScoredEntry {
            entry: make_file_entry(&format!("/proj/src/file{:02}.rs", i), 400, Some("rs")),
            score: (i as f32) * 0.05,
        })
        .collect();

    let config = WorksetConfig { max_files: 5, token_budget: 0, ..Default::default() };

    let ws1 = select(entries.clone(), &config);
    let ws2 = select(entries.clone(), &config);

    let paths1: Vec<_> = ws1.selected.iter().map(|f| f.path.clone()).collect();
    let paths2: Vec<_> = ws2.selected.iter().map(|f| f.path.clone()).collect();
    assert_eq!(paths1, paths2, "selection must be deterministic");
}

#[test]
fn workset_diversity_prevents_dir_domination() {
    // 10 files all in the same directory
    let entries: Vec<ScoredEntry> = (0..10)
        .map(|i| ScoredEntry {
            entry: make_file_entry(&format!("/proj/src/file{}.rs", i), 100, Some("rs")),
            score: 0.8,
        })
        .collect();

    let config = WorksetConfig {
        max_files: 100,
        token_budget: 0,
        max_per_dir: 3,
        ..Default::default()
    };
    let ws = select(entries, &config);
    assert_eq!(ws.selected.len(), 3, "at most max_per_dir files from one directory");
}

#[test]
fn workset_high_relevance_files_always_included() {
    // Mix of high and low score entries; verify high scores win
    let high_entry = ScoredEntry {
        entry: make_file_entry("/proj/src/critical.rs", 100, Some("rs")),
        score: 0.99,
    };
    let low_entries: Vec<ScoredEntry> = (0..20)
        .map(|i| ScoredEntry {
            entry: make_file_entry(&format!("/proj/misc/file{}.txt", i), 100, Some("txt")),
            score: 0.01,
        })
        .collect();

    let mut entries = vec![high_entry];
    entries.extend(low_entries);

    let config = WorksetConfig { max_files: 5, token_budget: 0, ..Default::default() };
    let ws = select(entries, &config);

    let paths: Vec<_> = ws.selected.iter().map(|f| f.path.clone()).collect();
    assert!(
        paths.contains(&PathBuf::from("/proj/src/critical.rs")),
        "high-relevance file should always be selected"
    );
}

#[test]
fn workset_respects_token_budget() {
    // Each file: 400 bytes → 100 tokens; budget = 250 → fits 2
    let entries: Vec<ScoredEntry> = (0..10)
        .map(|i| ScoredEntry {
            entry: make_file_entry(&format!("/p/f{}.rs", i), 400, Some("rs")),
            score: 0.5,
        })
        .collect();

    let config = WorksetConfig {
        max_files: 100,
        token_budget: 250,
        ..Default::default()
    };
    let ws = select(entries, &config);
    assert!(ws.estimated_tokens <= 250, "estimated tokens must not exceed budget");
    assert!(ws.selected.len() <= 3);
}

#[test]
fn workset_respects_min_score_filter() {
    let entries = vec![
        ScoredEntry { entry: make_file_entry("/p/a.rs", 100, Some("rs")), score: 0.7 },
        ScoredEntry { entry: make_file_entry("/p/b.rs", 100, Some("rs")), score: 0.3 },
        ScoredEntry { entry: make_file_entry("/p/c.rs", 100, Some("rs")), score: 0.9 },
    ];
    let config = WorksetConfig { min_score: 0.5, token_budget: 0, max_files: 100, ..Default::default() };
    let ws = select(entries, &config);
    assert_eq!(ws.selected.len(), 2);
    assert!(ws.selected.iter().all(|f| f.score >= 0.5));
}

// ────────────────────────────────────────────────────────────────────────────
// 6. Full pipeline integration
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn engine_build_context_end_to_end() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() { println!(\"hello\"); }\n").unwrap();
    fs::write(dir.path().join("src/lib.rs"), "pub fn add(a: i32, b: i32) -> i32 { a + b }\n").unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"\nversion = \"0.1.0\"\n").unwrap();

    let config = EngineConfig::new(dir.path());
    let engine = ContextEngine::new(config);

    let hints = PublicHints::default();
    let (packed, stats) = engine.build_context(hints, None).await.unwrap();

    assert!(stats.files_scored > 0, "should have scored some files");
    assert!(!packed.blocks.is_empty(), "should have at least one file block");
    assert!(packed.total_tokens > 0, "should have used some tokens");
    assert!(packed.total_tokens <= 32_000, "should be within budget");
}

#[tokio::test]
async fn engine_with_static_memory_includes_snippets() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();

    let snippets = vec![
        make_memory_snippet("notes.md", "Important: use async everywhere"),
        make_memory_snippet("arch.md", "The system uses a pipeline pattern"),
    ];
    let memory = Arc::new(StaticMemory::new(snippets));

    let config = EngineConfig::new(dir.path());
    let engine = ContextEngine::new(config).with_memory(memory);

    let hints = PublicHints {
        keywords: vec!["async".to_string()],
        ..Default::default()
    };
    let mq = Some(MemoryQuery {
        keywords: vec!["async".to_string()],
        total_token_budget: 500,
        ..Default::default()
    });

    let (packed, stats) = engine.build_context(hints, mq).await.unwrap();
    assert!(stats.memory_snippets > 0, "memory snippets should be retrieved");
    assert!(!packed.memory_snippets.is_empty(), "packed context should include memory snippets");
}

#[tokio::test]
async fn engine_rendered_context_has_file_markers() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("lib.rs"), "pub fn hello() -> &'static str { \"hi\" }\n").unwrap();

    let config = EngineConfig::new(dir.path());
    let engine = ContextEngine::new(config);

    let (packed, _) = engine.build_context(PublicHints::default(), None).await.unwrap();
    let rendered = packed.render();

    assert!(rendered.contains("// File:"), "rendered output should contain file markers");
}

#[tokio::test]
async fn engine_token_count_within_budget() {
    let dir = TempDir::new().unwrap();
    // Create several files
    for i in 0..5 {
        fs::write(
            dir.path().join(format!("file{}.rs", i)),
            format!("fn func_{i}() {{ let x = {i}; }}\n").repeat(20),
        ).unwrap();
    }

    let mut config = EngineConfig::new(dir.path());
    config.token_budget = 500;
    let engine = ContextEngine::new(config);

    let (packed, _) = engine.build_context(PublicHints::default(), None).await.unwrap();
    assert!(
        packed.total_tokens <= 500,
        "total_tokens {} should not exceed budget 500",
        packed.total_tokens
    );
}

#[tokio::test]
async fn engine_relevance_hints_keywords_boost_matching_files() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/auth.rs"), "pub fn authenticate() {}\n").unwrap();
    fs::write(dir.path().join("src/unrelated.rs"), "pub fn unrelated() {}\n").unwrap();

    let config = EngineConfig::new(dir.path());
    let engine = ContextEngine::new(config);

    let hints = PublicHints {
        keywords: vec!["auth".to_string()],
        ..Default::default()
    };
    let (packed, _) = engine.build_context(hints, None).await.unwrap();

    // auth.rs should appear in included files
    let included: Vec<_> = packed
        .included_files
        .iter()
        .filter_map(|p| p.file_name()?.to_str())
        .collect();
    assert!(included.contains(&"auth.rs"), "auth.rs should be included when 'auth' is a keyword");
}

// ────────────────────────────────────────────────────────────────────────────
// 7. Edge cases
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn engine_empty_directory_returns_error_not_panic() {
    let dir = TempDir::new().unwrap();
    // No files at all

    let config = EngineConfig::new(dir.path());
    let engine = ContextEngine::new(config);

    let result = engine.build_context(PublicHints::default(), None).await;
    match result {
        Err(ContextError::NoRelevantFiles(_)) => {} // expected
        Err(ContextError::EmptyWorkingSet) => {}     // also acceptable
        Err(other) => panic!("unexpected error variant: {:?}", other),
        Ok(_) => panic!("expected an error for empty directory"),
    }
}

#[tokio::test]
async fn engine_single_large_file_truncated_gracefully() {
    let dir = TempDir::new().unwrap();
    // File with ~2000 tokens worth of content
    let large_content = "fn foo() { let x = 1; }\n".repeat(350); // ~8400 chars, ~2100 tokens
    fs::write(dir.path().join("huge.rs"), &large_content).unwrap();

    let mut config = EngineConfig::new(dir.path());
    config.token_budget = 200; // much less than file size
    let engine = ContextEngine::new(config);

    let result = engine.build_context(PublicHints::default(), None).await;
    // Should succeed (truncate) rather than panic or error
    match result {
        Ok((packed, _)) => {
            assert!(packed.total_tokens <= 200, "should respect budget even for large files");
        }
        Err(ContextError::NoRelevantFiles(_)) | Err(ContextError::EmptyWorkingSet) => {
            // acceptable if file was excluded due to budget
        }
        Err(e) => panic!("unexpected error: {:?}", e),
    }
}

#[tokio::test]
async fn engine_all_files_ignored_returns_error() {
    let dir = TempDir::new().unwrap();
    // All real files are ignored; only the .gitignore itself is present (it's hidden → ignored)
    fs::write(dir.path().join(".gitignore"), "*.rs\n*.toml\n").unwrap();
    fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();

    let config = EngineConfig::new(dir.path());
    let engine = ContextEngine::new(config);

    let result = engine.build_context(PublicHints::default(), None).await;
    assert!(result.is_err(), "all files ignored should produce an error, not panic");
}

#[tokio::test]
async fn packer_empty_file_list_returns_empty_context() {
    let packer = ContextPacker::new(PackerConfig::default());
    let result = packer.pack(vec![], vec![]).await.unwrap();
    assert!(result.blocks.is_empty());
    assert_eq!(result.total_tokens, 0);
    assert!(!result.truncated);
}

#[test]
fn workset_empty_input_returns_empty_set() {
    let ws = select(vec![], &WorksetConfig::default());
    assert!(ws.selected.is_empty());
    assert!(ws.excluded.is_empty());
    assert_eq!(ws.estimated_tokens, 0);
}

#[tokio::test]
async fn scanner_respects_max_file_size_config() {
    let dir = TempDir::new().unwrap();
    // A file larger than the configured max
    let big = vec![b'a'; 2000];
    fs::write(dir.path().join("big.rs"), &big).unwrap();
    fs::write(dir.path().join("small.rs"), b"fn foo() {}").unwrap();

    let config = ScannerConfig {
        max_file_size_bytes: 500,
        ..Default::default()
    };
    let scanner = Scanner::new(dir.path()).with_config(config);
    let (entries, stats) = scanner.scan().await.unwrap();

    let names: Vec<_> = entries.iter().filter_map(|e| e.path.file_name()?.to_str()).collect();
    assert!(names.contains(&"small.rs"), "small file should be included");
    assert!(!names.contains(&"big.rs"), "oversized file should be excluded");
    assert!(stats.files_too_large >= 1);
}

#[tokio::test]
async fn vault_memory_loads_markdown_files() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("note1.md"), "The agent uses tools.").unwrap();
    fs::write(dir.path().join("note2.md"), "Memory is stored in vault.").unwrap();
    fs::write(dir.path().join("ignore.txt"), "not markdown").unwrap();

    let vault = VaultMemory::new(dir.path());
    let query = MemoryQuery {
        keywords: vec![],
        max_snippets: 10,
        total_token_budget: 10_000,
        ..Default::default()
    };
    let snippets = vault.retrieve(&query).await;

    assert_eq!(snippets.len(), 2, "only markdown files should be loaded");
    let sources: Vec<_> = snippets.iter().map(|s| s.source.as_str()).collect();
    assert!(sources.contains(&"note1.md"));
    assert!(sources.contains(&"note2.md"));
}

#[tokio::test]
async fn noop_memory_always_empty() {
    let mem = NoopMemory;
    let result = mem.retrieve(&MemoryQuery::default()).await;
    assert!(result.is_empty());
}

#[test]
fn relevance_score_capped_at_one() {
    // Combine all bonuses to verify we never exceed 1.0
    let entry = make_file_entry("/proj/Cargo.toml", 100, Some("toml"));
    let hints = RelevanceHints {
        keywords: vec!["cargo".to_string()],
        referenced_paths: vec![PathBuf::from("/proj/Cargo.toml")],
        root: Some(PathBuf::from("/proj")),
    };
    let s = score(&entry, &hints);
    assert!(s <= 1.0, "score should be capped at 1.0, got {}", s);
    assert!(s > 0.0);
}

#[test]
fn relevance_score_all_sorted_descending() {
    let entries = vec![
        make_file_entry("/proj/src/main.rs", 500, Some("rs")),
        make_file_entry("/proj/misc.txt", 5000, Some("txt")),
        make_file_entry("/proj/Cargo.toml", 300, Some("toml")),
    ];
    let hints = RelevanceHints { root: Some(PathBuf::from("/proj")), ..Default::default() };
    let scored = score_all(entries, &hints);
    for w in scored.windows(2) {
        assert!(
            w[0].score >= w[1].score,
            "score_all must return entries in descending order: {} < {}",
            w[0].score, w[1].score
        );
    }
}

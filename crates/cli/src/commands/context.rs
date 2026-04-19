//! `rustpi context` — show what context would be built for the current project.

use context_engine::{ContextEngine, EngineConfig, RelevanceHints};

pub async fn show_context(query: Option<String>) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let config = EngineConfig::new(&cwd);
    let engine = ContextEngine::new(config);

    let keywords: Vec<String> = query
        .as_deref()
        .map(|q| q.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();

    let hints = RelevanceHints {
        keywords,
        referenced_paths: vec![],
        root: Some(cwd),
    };

    println!("Scanning project...");
    match engine.build_context(hints, None).await {
        Ok((packed, stats)) => {
            println!(
                "Scanned {} files, scored {}, selected {}",
                stats.scan.files_found, stats.files_scored, stats.files_selected
            );
            println!(
                "Total tokens: {}, truncated: {}, compacted: {}",
                packed.total_tokens, packed.truncated, packed.compacted
            );
            println!("\nSelected files:");
            for block in &packed.blocks {
                println!(
                    "  {} ({} tokens{})",
                    block.path.display(),
                    block.actual_tokens,
                    if block.truncated { ", truncated" } else { "" }
                );
            }
            if let Some(summary) = &packed.compact_summary {
                let preview_len = summary.len().min(200);
                println!("\nCompact summary: {}", &summary[..preview_len]);
            }
        }
        Err(e) => {
            eprintln!("Context engine error: {}", e);
        }
    }
    Ok(())
}

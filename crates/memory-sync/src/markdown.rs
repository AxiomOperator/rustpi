//! Safe, deterministic Markdown I/O for vault documents.

use std::collections::HashMap;

use crate::error::MemorySyncError;

/// A parsed Markdown document with optional YAML frontmatter.
pub struct VaultDoc {
    /// Simple key-value frontmatter (YAML subset).
    pub frontmatter: Option<HashMap<String, String>>,
    /// Ordered sections split on `#` headings.
    pub sections: Vec<DocSection>,
    /// Original text preserved for safety.
    pub raw: String,
}

/// A single section within a vault document.
pub struct DocSection {
    /// `None` for top-level prose before the first heading.
    pub heading: Option<String>,
    /// Heading level 1-6; 0 for pre-heading prose.
    pub level: u8,
    /// Full section body including any `<!-- machine-managed -->` tag.
    pub content: String,
    /// True when the body contains `<!-- machine-managed -->`.
    pub machine_managed: bool,
}

const MACHINE_TAG: &str = "<!-- machine-managed -->";

impl VaultDoc {
    /// Parse markdown text into sections. Never panics.
    pub fn parse(text: &str) -> Result<Self, MemorySyncError> {
        let raw = text.to_string();
        // Use split('\n') so that a trailing newline is preserved as a final "".
        let lines: Vec<&str> = text.split('\n').collect();
        let mut i = 0;

        // ── Frontmatter ──────────────────────────────────────────────────────
        let mut frontmatter: Option<HashMap<String, String>> = None;
        if !lines.is_empty() && lines[0] == "---" {
            i = 1;
            let mut fm_map = HashMap::new();
            let mut closed = false;
            while i < lines.len() {
                if lines[i] == "---" {
                    i += 1;
                    closed = true;
                    break;
                }
                let line = lines[i];
                if let Some(colon) = line.find(':') {
                    let key = line[..colon].trim().to_string();
                    let val = line[colon + 1..].trim().to_string();
                    fm_map.insert(key, val);
                } else if !line.trim().is_empty() {
                    return Err(MemorySyncError::MalformedMarkdown(
                        "frontmatter".to_string(),
                    ));
                }
                i += 1;
            }
            if !closed {
                return Err(MemorySyncError::MalformedMarkdown(
                    "unclosed frontmatter".to_string(),
                ));
            }
            frontmatter = Some(fm_map);
        }

        // ── Sections ─────────────────────────────────────────────────────────
        let mut sections = Vec::new();
        let mut cur_heading: Option<String> = None;
        let mut cur_level: u8 = 0;
        let mut cur_lines: Vec<&str> = Vec::new();

        while i < lines.len() {
            let line = lines[i];
            if line.starts_with('#') {
                let level = line.chars().take_while(|&c| c == '#').count() as u8;
                // Only treat as heading if the '#' run is followed by a space or end.
                let rest = &line[level as usize..];
                if rest.is_empty() || rest.starts_with(' ') {
                    let heading_text = rest.trim().to_string();
                    push_section(
                        &mut sections,
                        cur_heading.take(),
                        cur_level,
                        cur_lines.join("\n"),
                    );
                    cur_heading = Some(heading_text);
                    cur_level = level;
                    cur_lines = Vec::new();
                    i += 1;
                    continue;
                }
            }
            cur_lines.push(line);
            i += 1;
        }
        // Flush final section.
        push_section(
            &mut sections,
            cur_heading,
            cur_level,
            cur_lines.join("\n"),
        );

        Ok(Self { frontmatter, sections, raw })
    }

    /// Serialize back to Markdown deterministically.
    pub fn render(&self) -> String {
        let mut out = String::new();

        if let Some(fm) = &self.frontmatter {
            out.push_str("---\n");
            let mut pairs: Vec<(&String, &String)> = fm.iter().collect();
            pairs.sort_by_key(|(k, _)| k.as_str());
            for (k, v) in pairs {
                out.push_str(k);
                out.push_str(": ");
                out.push_str(v);
                out.push('\n');
            }
            out.push_str("---\n");
        }

        for section in &self.sections {
            match &section.heading {
                None => {
                    out.push_str(&section.content);
                }
                Some(h) => {
                    let hashes = "#".repeat(section.level as usize);
                    out.push_str(&hashes);
                    out.push(' ');
                    out.push_str(h);
                    out.push('\n');
                    out.push_str(&section.content);
                }
            }
        }

        out
    }

    /// Get a section by heading name (case-insensitive).
    pub fn section(&self, heading: &str) -> Option<&DocSection> {
        let lower = heading.to_lowercase();
        self.sections.iter().find(|s| {
            s.heading.as_deref().map(|h| h.to_lowercase()) == Some(lower.clone())
        })
    }

    /// Replace or insert a machine-managed section by heading.
    ///
    /// `body` is the new content **excluding** the `<!-- machine-managed -->` tag.
    /// Human-authored sections are never overwritten.
    pub fn upsert_machine_section(&mut self, heading: &str, body: &str) {
        let lower = heading.to_lowercase();
        for section in &mut self.sections {
            if section.heading.as_deref().map(|h| h.to_lowercase())
                == Some(lower.clone())
            {
                if section.machine_managed {
                    // Preserve content up-to-and-including the tag, replace the rest.
                    if let Some(tag_pos) = section.content.find(MACHINE_TAG) {
                        let end = tag_pos + MACHINE_TAG.len();
                        let trimmed = body.trim_end_matches('\n');
                        section.content =
                            format!("{}\n{}\n", &section.content[..end], trimmed);
                    } else {
                        // Tag vanished somehow; restore it.
                        let trimmed = body.trim_end_matches('\n');
                        section.content = format!("{}\n{}\n", MACHINE_TAG, trimmed);
                        section.machine_managed = true;
                    }
                }
                // Non-machine sections are left untouched.
                return;
            }
        }

        // Not found — insert a new machine-managed section.
        let trimmed = body.trim_end_matches('\n');
        self.sections.push(DocSection {
            heading: Some(heading.to_string()),
            level: 2,
            content: format!("{}\n{}\n", MACHINE_TAG, trimmed),
            machine_managed: true,
        });
    }

    /// Return a frontmatter value by key.
    pub fn frontmatter_value(&self, key: &str) -> Option<&str> {
        self.frontmatter.as_ref()?.get(key).map(String::as_str)
    }
}

/// Push a section into `sections`, skipping vacuous pre-heading content.
fn push_section(
    sections: &mut Vec<DocSection>,
    heading: Option<String>,
    level: u8,
    content: String,
) {
    // Skip a pre-heading block that contains nothing but whitespace.
    if heading.is_none() && content.chars().all(|c| c.is_whitespace()) {
        return;
    }
    let machine_managed = content.contains(MACHINE_TAG);
    sections.push(DocSection { heading, level, content, machine_managed });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_no_sections() {
        let doc = VaultDoc::parse("").unwrap();
        assert!(doc.sections.is_empty());
        assert!(doc.frontmatter.is_none());
    }

    #[test]
    fn frontmatter_parsed() {
        let text = "---\ntitle: Hello World\nauthor: Alice\n---\n# Doc\nBody.\n";
        let doc = VaultDoc::parse(text).unwrap();
        let fm = doc.frontmatter.as_ref().unwrap();
        assert_eq!(fm.get("title").map(String::as_str), Some("Hello World"));
        assert_eq!(fm.get("author").map(String::as_str), Some("Alice"));
    }

    #[test]
    fn headings_create_sections() {
        let text = "# First\nContent A\n## Second\nContent B\n";
        let doc = VaultDoc::parse(text).unwrap();
        assert_eq!(doc.sections.len(), 2);
        assert_eq!(doc.sections[0].heading.as_deref(), Some("First"));
        assert_eq!(doc.sections[0].level, 1);
        assert_eq!(doc.sections[1].heading.as_deref(), Some("Second"));
        assert_eq!(doc.sections[1].level, 2);
    }

    #[test]
    fn machine_managed_flag_detected() {
        let text = "# HB\n<!-- machine-managed -->\nStatus: idle\n";
        let doc = VaultDoc::parse(text).unwrap();
        assert_eq!(doc.sections.len(), 1);
        assert!(doc.sections[0].machine_managed);
    }

    #[test]
    fn upsert_updates_existing_machine_section() {
        let text = "# HB\n<!-- machine-managed -->\nStatus: idle\n";
        let mut doc = VaultDoc::parse(text).unwrap();
        doc.upsert_machine_section("HB", "Status: running");
        let section = doc.section("HB").unwrap();
        assert!(section.machine_managed);
        assert!(section.content.contains("Status: running"));
        assert!(!section.content.contains("Status: idle"));
    }

    #[test]
    fn upsert_adds_new_section_if_absent() {
        let text = "# Existing\nHuman content\n";
        let mut doc = VaultDoc::parse(text).unwrap();
        doc.upsert_machine_section("NewSection", "auto content");
        assert_eq!(doc.sections.len(), 2);
        let sec = doc.section("NewSection").unwrap();
        assert!(sec.machine_managed);
        assert!(sec.content.contains("auto content"));
    }

    #[test]
    fn upsert_does_not_touch_human_section() {
        let text = "# Notes\nHuman wrote this\n";
        let mut doc = VaultDoc::parse(text).unwrap();
        // Try to upsert a section that exists but is human-authored.
        doc.upsert_machine_section("Notes", "machine wants to write");
        // Human content must be preserved.
        let sec = doc.section("Notes").unwrap();
        assert!(!sec.machine_managed);
        assert!(sec.content.contains("Human wrote this"));
    }

    #[test]
    fn render_round_trips() {
        let text = "# Soul\n\nCore rules and ethics.\n";
        let doc = VaultDoc::parse(text).unwrap();
        assert_eq!(doc.render(), text);
    }

    #[test]
    fn render_round_trips_with_single_key_frontmatter() {
        let text = "---\nkey: value\n---\n# Section\n\nBody.\n";
        let doc = VaultDoc::parse(text).unwrap();
        assert_eq!(doc.render(), text);
    }

    #[test]
    fn malformed_frontmatter_no_colon() {
        let text = "---\nbadline\n---\n";
        let result = VaultDoc::parse(text);
        assert!(matches!(result, Err(MemorySyncError::MalformedMarkdown(_))));
    }

    #[test]
    fn malformed_frontmatter_unclosed() {
        let text = "---\nkey: value\n";
        let result = VaultDoc::parse(text);
        assert!(matches!(result, Err(MemorySyncError::MalformedMarkdown(_))));
    }
}

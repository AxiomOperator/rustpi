//! Output formatting helpers for print and JSON modes.

use std::io::{IsTerminal, Write};

use serde_json::Value;

use crate::args::OutputFormat;

/// Routes output to human-readable or machine-readable JSON modes.
pub struct Output {
    pub format: OutputFormat,
    /// Whether ANSI color codes are active (print mode + TTY only).
    #[allow(dead_code)]
    pub color: bool,
}

impl Output {
    /// Create an `Output` instance, auto-detecting TTY and honoring `NO_COLOR`.
    pub fn new(format: OutputFormat, config_color: bool) -> Self {
        let is_tty = std::io::stdout().is_terminal();
        let no_color = std::env::var("NO_COLOR").is_ok()
            || std::env::var("TERM").ok().as_deref() == Some("dumb");
        let color = is_tty && !no_color && config_color && matches!(format, OutputFormat::Print);
        Self { format, color }
    }

    /// Print a token delta immediately (streaming); flushes stdout after each chunk.
    pub fn print_token(&self, delta: &str) {
        print!("{}", delta);
        let _ = std::io::stdout().flush();
    }

    /// Print a successful result.
    /// In JSON mode writes `{"ok":true,"data":{...}}` to stdout.
    pub fn print_success(&self, label: &str, data: &Value) {
        match &self.format {
            OutputFormat::Print => {
                if !label.is_empty() {
                    println!("\n{}", label);
                }
                print_value_pretty(data);
            }
            OutputFormat::Json => {
                println!("{}", serde_json::json!({"ok": true, "data": data}));
                let _ = std::io::stdout().flush();
            }
        }
    }

    /// Emit a JSONL streaming event line to stdout (JSON mode only).
    pub fn emit_json_line(&self, event: &str, data: Value) {
        if matches!(self.format, OutputFormat::Json) {
            println!("{}", serde_json::json!({"event": event, "data": data}));
            let _ = std::io::stdout().flush();
        }
    }

    /// Emit the terminal `{"event":"done",...}` line (JSON mode only).
    pub fn emit_json_done(&self, data: Value) {
        if matches!(self.format, OutputFormat::Json) {
            println!("{}", serde_json::json!({"event": "done", "data": data}));
            let _ = std::io::stdout().flush();
        }
    }

    /// Print an error to stderr in both modes.
    pub fn print_err(&self, message: &str) {
        eprintln!("error: {}", message);
    }

    /// Print an informational lifecycle annotation to stderr (print mode only).
    pub fn print_info(&self, message: &str) {
        if matches!(self.format, OutputFormat::Print) {
            eprintln!("[info] {}", message);
        }
    }

    /// Print a section header (print mode only).
    pub fn print_header(&self, title: &str) {
        if matches!(self.format, OutputFormat::Print) {
            println!("\n{}", title);
            println!("{}", "─".repeat(title.len()));
        }
    }

    /// Print a key-value row (print mode only).
    pub fn print_kv(&self, key: &str, value: &str) {
        if matches!(self.format, OutputFormat::Print) {
            println!("  {:<22} {}", key, value);
        }
    }

    /// Print an empty line (print mode only).
    pub fn print_blank(&self) {
        if matches!(self.format, OutputFormat::Print) {
            println!();
        }
    }
}

/// Recursively pretty-print a JSON value as key-value pairs.
fn print_value_pretty(value: &Value) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                match v {
                    Value::String(s) => println!("  {:<22} {}", k, s),
                    Value::Bool(b) => println!("  {:<22} {}", k, b),
                    Value::Number(n) => println!("  {:<22} {}", k, n),
                    Value::Null => println!("  {:<22} -", k),
                    Value::Array(arr) => {
                        let joined = arr
                            .iter()
                            .map(|v| v.as_str().unwrap_or(&v.to_string()).to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        println!("  {:<22} {}", k, joined);
                    }
                    _ => println!("  {:<22} {}", k, v),
                }
            }
        }
        _ => println!("{}", value),
    }
}

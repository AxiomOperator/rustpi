# rustpi — Command Examples

## Basic usage

```bash
# Run with an inline prompt
rustpi run "explain how async/await works in Rust"

# Run with no arguments (reads from stdin interactively)
rustpi run

# Pipe a prompt from stdin
echo "what is a monad?" | rustpi run
```

## Reading from a file

```bash
# Read prompt from a file
rustpi run --file prompt.txt

# Pipe file contents
cat prompt.txt | rustpi run
```

## Provider and model overrides

```bash
# Use a specific provider
rustpi run --provider openai "summarise this codebase"

# Use a specific model
rustpi run --model gpt-4o "review this function"

# Combine provider and model
rustpi run --provider openai --model gpt-4o "write a unit test for src/lib.rs"
```

## Output format

```bash
# Default human-readable streaming output
rustpi run "hello"

# Machine-readable JSONL (one event per line)
rustpi run --output json "hello"
```

## Session management

```bash
# Start a new run and note the session UUID from the output, then continue it
rustpi run --session-id <uuid> "follow-up question"
```

## Non-interactive / scripting

```bash
# Fail immediately if no prompt is provided (safe for scripts)
rustpi run --non-interactive "what is 2 + 2"

# Exit non-zero when stdin is not a tty and no prompt is given
rustpi run --non-interactive
```

## Config override

```bash
# Use an alternate config file
rustpi run --config ~/my-config.toml "hello"
```

---

## TUI (Terminal User Interface)

```bash
# Launch the interactive full-screen TUI
rustpi-tui

# Launch with a custom config
RUSTPI_CONFIG=~/my-config.toml rustpi-tui

# Build and run the TUI directly via cargo
cargo run --bin rustpi-tui
```

### TUI keybindings

| Key | Action |
|-----|--------|
| `Enter` | Submit input / send message |
| `q` / `Ctrl+C` | Quit |
| `1`–`6` | Switch pane (Conversation, Tools, Context, Sessions, Auth, Logs) |
| `↑` / `k` | Scroll up |
| `↓` / `j` | Scroll down |
| `PgUp` / `PgDn` | Page up / page down |
| `Ctrl+I` | Interrupt current request |
| `y` / `n` | Approve / deny tool action |
| `Backspace` | Delete character |
| `?` | Help |

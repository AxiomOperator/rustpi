# CLI & TUI Test Checklist

Operator-facing checklist for validating the `rustpi` CLI and TUI. Items marked **Automated** reference tests in `crates/cli/tests/cli_tests.rs` or `crates/tui/tests/tui_tests.rs`. Items marked **Manual** require a running binary and/or live provider.

---

## CLI Checklist

### Help / Version Flags

- [ ] **Help flag**: Run `rustpi --help`; verify usage text contains "rustpi" and lists subcommands. **Automated** → `test_help_flag`
- [ ] **Run help**: Run `rustpi run --help`; verify output contains "run". **Automated** → `test_run_help`
- [ ] **No command prints help**: Run `rustpi` with no arguments; verify exit 0 and help text is printed. **Automated** → `test_no_command_prints_help`
- [ ] **Version flag**: Run `rustpi --version`; verify semver string is printed and process exits 0. **Manual**
- [ ] **Invalid output format**: Run `rustpi --output badval`; verify exit code 2 (clap arg error). **Automated** → `test_invalid_output_format`

---

### `run` Command: Prompt Submission & Streaming Output

- [ ] **Basic prompt**: Run `rustpi run "hello"`; verify process exits 0 and streaming token output appears on stdout. **Automated** → `test_run_with_prompt`
- [ ] **Streaming tokens appear incrementally**: Run `rustpi run "describe the sky"` against a live provider; observe tokens are printed as they arrive, not all at once. **Manual**
- [ ] **Task execution completes**: Run a multi-step task prompt; verify run reaches `run_completed` and process exits 0. **Manual**
- [ ] **No-prompt non-interactive**: Run `rustpi run --non-interactive`; verify exit code 2 and an `InvalidArgs` error message. **Automated** → `test_run_non_interactive_no_prompt`

---

### `run --file`: File-Based Prompt Execution

- [ ] **File prompt succeeds**: Create a temp file containing prompt text; run `rustpi run --file <path>`; verify exit 0. **Automated** → `test_run_from_file`
- [ ] **Missing file errors cleanly**: Run `rustpi run --file /nonexistent/path.md`; verify non-zero exit (code 1) and IO error message on stderr. **Automated** → `test_run_with_file_flag`

---

### `run --output json`: JSON Output Mode

- [ ] **JSON envelope shape**: Run `rustpi --output json run "hello"`; verify stdout is valid JSON with `{"ok": true, "data": ...}` structure. **Manual** (build on `test_output_json_success` unit test)
- [ ] **JSON error envelope**: Run a failing command in JSON mode; verify stdout contains `{"ok": false, "error": "..."}`. **Manual**

---

### `run --model`: Model Flag Override

- [ ] **Model flag accepted**: Run `rustpi --model gpt-4o run "hello"`; verify exit 0 (simulation uses the flag without error). **Automated** → `test_run_with_model_flag`
- [ ] **Provider flag accepted**: Run `rustpi --provider openai run "hello"`; verify exit 0. **Automated** → `test_run_with_provider_flag`
- [ ] **Unknown model rejected by provider**: Against a live provider, run `rustpi --model nonexistent-model-xyz run "test"`; verify non-zero exit and an informative error. **Manual**

---

### `session list`: List Sessions

- [ ] **Session list exits 0**: Run `rustpi session list`; verify exit 0 and non-empty output (header line or "(none)"). **Automated** → `test_session_list`
- [ ] **Session list JSON**: Run `rustpi --output json session list`; verify valid JSON with `{"ok": true, "data": [...]}`. **Automated** → `test_session_list_json`
- [ ] **Session list after creating sessions**: Create two sessions via the executor library; verify both IDs appear in the list. **Automated** → `test_executor_session_list`

---

### `session resume <id>`: Resume a Previous Session

- [ ] **Session attach/detach lifecycle**: Create a session, detach it; verify `status` transitions to "ended". **Automated** → `test_executor_session_attach_detach`
- [ ] **Session not found returns error**: Attempt `session_info` on a nonexistent UUID; verify `SessionNotFound` error. **Automated** → `test_executor_session_info_not_found`
- [ ] **Resume via CLI**: After a process restart, run `rustpi session resume <id>` for a persisted session; verify the session is reattached and previous history is visible. **Manual** (requires persistent backend)

---

### `auth login`: OAuth / Device-Auth Flow

- [ ] **Device-auth flow**: Run `rustpi auth login --provider copilot`; verify user code and verification URL are printed; complete auth in browser and confirm token is stored. **Manual**
- [ ] **OAuth browser flow**: Run `rustpi auth login --provider gemini` (when configured); verify browser opens and token persists after redirect. **Manual**
- [ ] **API key stored**: Configure `OPENAI_API_KEY` env var; verify `rustpi auth status` shows the key is active. **Manual**

---

### `auth status`: Token Validity Check

- [ ] **Auth status exits 0**: Run `rustpi auth status`; verify exit 0 and a status message. **Automated** → `test_auth_status`
- [ ] **Unauthenticated shows disconnected**: With no token configured, `auth status` should show "not authenticated" or equivalent (stub always returns `authenticated=false`). **Automated** → `test_auth_status`
- [ ] **Authenticated shows token info**: After completing auth, `auth status` should show the provider name and token expiry. **Manual**

---

### `diag`: Diagnostics Output

- [ ] **Diag exits 0**: Run `rustpi diag`; verify exit 0 and non-empty stdout with multiple sections. **Automated** → `test_diag`
- [ ] **Diag JSON exits 0**: Run `rustpi --output json diag`; verify non-empty stdout. **Automated** → `test_diag_json`
- [ ] **Diag JSON is valid**: Run `rustpi --output json diag`; parse stdout as JSON; verify `{"ok": true, "data": {...}}`. **Automated** → `test_diag_json_valid`
- [ ] **Diag surfaces provider config**: Confirm that provider IDs, model defaults, and auth status appear in diag output. **Manual**

---

### `replay` Command

- [ ] **Replay exits 0**: Run `rustpi replay`; verify exit 0. **Automated** → `test_replay_command_exits_zero`
- [ ] **Replay --audit-only**: Run `rustpi replay --audit-only`; verify exit 0. **Automated** → `test_replay_audit_flag`
- [ ] **Replay --failures-only**: Run `rustpi replay --failures-only`; verify exit 0. **Automated** → `test_replay_failures_flag`
- [ ] **Replay JSON output**: Run `rustpi --output json replay`; verify stdout contains `"ok"` JSON key. **Automated** → `test_replay_json_output`
- [ ] **Replay help**: Run `rustpi replay --help`; verify "replay" appears in output. **Automated** → `test_replay_help`

---

### Piped Input: `echo "task" | rustpi run`

- [ ] **Piped stdin prompt**: Run `echo "summarise this" | rustpi run`; verify the piped text is used as the prompt and process exits 0. **Manual**
- [ ] **Piped stdin with --non-interactive**: Run `echo "task" | rustpi run --non-interactive`; verify the combined piped+flag flow succeeds. **Manual**

---

### Error Exit Codes

- [ ] **Non-zero on IO error**: `rustpi run --file /missing`  → exit code 1. **Automated** → `test_run_with_file_flag`
- [ ] **Exit code 2 on bad args**: `rustpi --output badval` → exit code 2. **Automated** → `test_invalid_output_format`
- [ ] **Exit code 2 on missing required arg**: `rustpi run --non-interactive` (no prompt) → exit code 2. **Automated** → `test_run_non_interactive_no_prompt`
- [ ] **Zero on clean runs**: All success paths exit 0. **Automated** (covered across multiple tests above)

---

## TUI Checklist

> TUI tests use `ratatui::backend::TestBackend` and the `apply_agent_event` / `map_key` / `compute_layout` / pane render APIs. They do not require a live terminal.

### Launch

- [ ] **TUI binary launches**: Run `rustpi-tui`; verify the terminal UI renders without panic. **Manual**
- [ ] **Default state is empty**: On startup, `AppState::default()` has no sessions, no messages, no pending approvals. **Automated** → `test_state_default_is_empty`

---

### Session List Navigation

- [ ] **Session pane renders sessions**: After injecting `SessionCreated` events, the session pane lists session IDs and status. **Automated** → `test_session_pane_renders_sessions`, `test_apply_session_created`
- [ ] **Up/down navigation changes focus**: Pressing key `1` focuses conversation pane; key `2` focuses tools pane; navigation between panes updates `AppState.focused_pane`. **Automated** → `test_key_1_maps_to_focus_conversation`, `test_key_2_maps_to_focus_tools`, `test_focus_changes_on_key`
- [ ] **Session pane navigation (manual)**: With multiple sessions loaded, use arrow keys to move selection up/down and verify highlighted session changes. **Manual**

---

### Chat History Scrolling

- [ ] **Conversation pane renders messages**: Inject `AssistantMessage` events; verify messages appear in conversation pane render output. **Automated** → `test_conversation_pane_renders_messages`
- [ ] **Scroll down**: Key `j` maps to `ScrollDown` action. **Automated** → `test_key_j_maps_to_scroll_down`
- [ ] **Scroll up**: Key `k` maps to `ScrollUp` action. **Automated** → `test_key_k_maps_to_scroll_up`
- [ ] **Long history scrolling (manual)**: With 50+ messages, scroll to bottom and top; verify scroll offset is bounded correctly. **Manual**

---

### Streaming Token Display

- [ ] **Token chunk accumulates**: Applying multiple `TokenChunk` events appends content to the in-progress message. **Automated** → `test_apply_token_chunk_accumulates`
- [ ] **Streaming message flushed on RunCompleted**: After `RunCompleted`, the streaming buffer is committed as a final message. **Automated** → `test_apply_run_completed_flushes_streaming`
- [ ] **Tokens appear incrementally (manual)**: During a live run, observe that each token chunk updates the conversation pane in real time without waiting for run completion. **Manual**

---

### Approval / Interrupt Flow

- [ ] **Approval dialog shown**: When a `ToolApprovalRequired` event is applied, `AppState.pending_approval` is set and the approval UI is visible in the conversation pane render. **Automated** → `test_approval_shown_in_conversation`
- [ ] **Approve key (y)**: Key `y` maps to `Approve` action. **Automated** → `test_key_y_maps_to_approve`
- [ ] **Deny key (n)**: Key `n` maps to `Deny` action. **Automated** → `test_key_n_maps_to_deny`
- [ ] **Approve clears pending approval**: Applying an `Approve` action removes `pending_approval` from state. **Automated** → `test_approve_clears_pending_approval`
- [ ] **Deny clears pending approval**: Applying a `Deny` action removes `pending_approval` from state. **Automated** → `test_deny_clears_pending_approval`
- [ ] **Interrupt clears active run**: Applying `Interrupt` removes the active run from state. **Automated** → `test_interrupt_clears_active_run`
- [ ] **Interrupt key (ctrl-i)**: Key `Ctrl+i` maps to `Interrupt` action. **Automated** → `test_ctrl_i_maps_to_interrupt`
- [ ] **Approval outcome reflected in UI (manual)**: During a live run that requires shell approval, approve and observe the tool executes; deny and observe the run aborts. **Manual**

---

### Provider Status Pane

- [ ] **Auth pane renders providers**: Inject `AuthStateChanged` events; verify the auth/provider pane shows provider IDs and connection status. **Automated** → `test_auth_pane_renders_providers`, `test_apply_auth_state_changed`
- [ ] **Provider status pane (manual)**: Verify configured providers are listed with their model and auth status in the live TUI. **Manual**

---

### Auth Status

- [ ] **Auth state change applied**: `AuthStateChanged` event updates `AppState.provider_statuses`. **Automated** → `test_apply_auth_state_changed`
- [ ] **Connected/disconnected display (manual)**: After `auth login`, relaunch TUI; verify provider shows as connected with token info. **Manual**

---

### Log / Diagnostics Pane

- [ ] **Logs pane renders entries**: Inject log entries into state; verify log pane renders them. **Automated** → `test_logs_pane_renders_entries`
- [ ] **Log entries capped**: After inserting 600 log entries, only the most recent 500 are retained. **Automated** → `test_log_entries_capped_at_500`
- [ ] **Tool events capped**: After inserting 250 tool events, only the most recent 200 are retained. **Automated** → `test_tool_events_capped_at_200`

---

### Tool Activity Pane

- [ ] **Tools pane renders activity**: Inject `ToolExecutionStarted` and `ToolStdout` events; verify tool pane renders tool name and output. **Automated** → `test_tools_pane_renders_activity`, `test_apply_tool_execution_started`, `test_apply_tool_stdout`, `test_apply_tool_completed`
- [ ] **Tools pane empty state**: With no tool activity, tools pane renders without error. **Automated** → `test_tools_pane_renders_empty`

---

### Keyboard Shortcuts

- [ ] **Quit (q)**: Key `q` maps to `Quit` action. **Automated** → `test_key_q_maps_to_quit`
- [ ] **Quit (ctrl-c)**: Key `Ctrl+c` maps to `Quit` action. **Automated** → `test_ctrl_c_maps_to_quit`
- [ ] **Focus conversation (1)**: Key `1` maps to `FocusConversation`. **Automated** → `test_key_1_maps_to_focus_conversation`
- [ ] **Focus tools (2)**: Key `2` maps to `FocusTools`. **Automated** → `test_key_2_maps_to_focus_tools`
- [ ] **Submit (Enter)**: Key `Enter` maps to `Submit`. **Automated** → `test_enter_maps_to_submit`
- [ ] **Type character**: A printable key maps to `TypeChar`. **Automated** → `test_char_a_maps_to_type_char`
- [ ] **All shortcuts work in live TUI (manual)**: Verify each key binding produces the expected pane navigation or action in the running terminal UI. **Manual**

---

### Layout

- [ ] **Layout produces 6 rects**: `compute_layout` on a standard terminal returns exactly 6 non-zero pane rectangles. **Automated** → `test_layout_produces_6_rects`
- [ ] **No overlap between panes**: The 6 layout rects do not overlap each other. **Automated** → `test_layout_no_overlap`
- [ ] **Layout fits within terminal**: All pane rects are contained within terminal bounds. **Automated** → `test_layout_fits_within_terminal`
- [ ] **Small terminal handled**: `compute_layout` on a very small terminal (e.g. 10×5) does not panic. **Automated** → `test_layout_handles_small_terminal`
- [ ] **Context pane renders**: With and without context info, context pane renders without panic. **Automated** → `test_context_pane_no_context`, `test_context_pane_with_context`
- [ ] **Conversation pane empty state**: Conversation pane renders without messages present. **Automated** → `test_conversation_pane_renders_empty`

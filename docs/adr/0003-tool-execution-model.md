# ADR-0003: Tool Execution Model

**Status:** Accepted

## Context

The agent calls tools requested by the model. Tools may be:
- Internal (file read/write, search)
- External (arbitrary subprocesses)
- Sensitive (shell execution, network calls, file mutation)

Requirements:
- All tools must emit structured lifecycle events
- Sensitive tools must be gateable by policy and user approval
- Timeouts and cancellation must be reliable
- Path traversal and unsafe argument patterns must be blocked

## Decision

All tool execution flows through `tool-runtime::runner::ToolRunner`:

1. **Tool registration:** Tools are registered by name in `ToolRegistry` with a JSON Schema for their parameters. The registry's `schemas()` method returns all schemas for injection into model requests.

2. **Unified `Tool` trait:**
```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn schema(&self) -> serde_json::Value;
    async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolError>;
}
```

3. **Timeout enforcement:** `ToolRunner::execute` wraps every call in `tokio::time::timeout`. Default timeout is configurable; tools may not override it upward.

4. **Cancellation:** A `tokio_util::CancellationToken` is passed into subprocess-based tools (Phase 5). SIGTERM is sent on cancellation, followed by SIGKILL after a grace period.

5. **Policy gate:** Before execution, `policy-engine::PolicyEngine::evaluate_tool` is consulted. `PolicyVerdict::Deny` returns `ToolError::PolicyDenied` immediately. `PolicyVerdict::RequireApproval` suspends execution and emits a `ToolApprovalRequired` event (Phase 5) for the UI to surface.

6. **Event emission:** The runner emits `ToolExecutionStarted`, `ToolStdout`, `ToolStderr`, and one terminal event per execution via the `AgentEvent` bus.

7. **Path safety:** File-path arguments are resolved to absolute paths and checked against an allowlist of working-directory prefixes before execution.

## Consequences

**Positive:**
- Every tool invocation is fully auditable through events
- Policy and approval are enforced at a single choke point
- New tools require only implementing the `Tool` trait

**Negative:**
- The JSON Schema validation overhead per call is non-zero
- Subprocess stdout/stderr streaming requires `tokio::process` and careful buffer management (Phase 5)
- Approval workflows require a round-trip to the UI/operator, adding latency

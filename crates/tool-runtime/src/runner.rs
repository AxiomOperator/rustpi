//! Tool runner: dispatches tool calls through the registry with safety enforcement.
//!
//! Phase 0 stub — full subprocess execution deferred to Phase 5.

use crate::{ToolCall, ToolError, ToolResult, registry::ToolRegistry};
use std::sync::Arc;
use std::time::Duration;

pub struct ToolRunner {
    registry: Arc<ToolRegistry>,
    default_timeout: Duration,
}

impl ToolRunner {
    pub fn new(registry: Arc<ToolRegistry>, default_timeout: Duration) -> Self {
        Self { registry, default_timeout }
    }

    pub async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        let tool = self
            .registry
            .get(&call.name)
            .ok_or_else(|| ToolError::NotFound(call.name.clone()))?;

        tokio::time::timeout(self.default_timeout, tool.execute(call))
            .await
            .map_err(|_| ToolError::Timeout(self.default_timeout.as_secs()))?
            .map_err(Into::into)
    }
}

//! Tool registry: maps tool names to their schemas and executors.

use crate::{ToolCall, ToolError, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// A registered tool.
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    /// JSON Schema for the tool's parameters.
    fn schema(&self) -> Value;
    async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolError>;
}

/// Registry of available tools.
#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    /// Return JSON Schema objects for all registered tools.
    pub fn schemas(&self) -> Vec<Value> {
        self.tools.values().map(|t| t.schema()).collect()
    }
}

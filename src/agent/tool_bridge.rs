//! Tool bridge - adapts OpenAgent tools to rig-core's Tool trait

use crate::tools::Tool;
use rig::completion::ToolDefinition;
use rig::tool::{Tool as RigTool, ToolError, ToolSet};
use serde::Deserialize;
use std::sync::Arc;

/// Arguments for tool calls (generic JSON)
#[derive(Deserialize)]
pub struct ToolArgs {
    #[serde(flatten)]
    pub args: serde_json::Value,
}

/// Adapter that wraps an OpenAgent tool and implements rig's Tool trait
pub struct RigToolAdapter {
    /// The wrapped OpenAgent tool
    tool: Arc<dyn Tool>,
}

impl RigToolAdapter {
    /// Create a new adapter for an OpenAgent tool
    pub fn new(tool: Arc<dyn Tool>) -> Self {
        Self { tool }
    }
}

impl RigTool for RigToolAdapter {
    const NAME: &'static str = "openagent_tool_adapter";

    type Error = ToolError;
    type Args = ToolArgs;
    type Output = serde_json::Value;

    fn name(&self) -> String {
        self.tool.name().to_string()
    }

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let def = self.tool.to_definition();
        ToolDefinition {
            name: def.function.name,
            description: def.function.description,
            parameters: def.function.parameters,
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // Pass the JSON value directly to the OpenAgent tool
        let result = self.tool.execute(args.args).await
            .map_err(|e| ToolError::ToolCallError(Box::new(e)))?;

        // Convert result to JSON
        if result.success {
            if let Some(content) = result.content {
                // Try to parse as JSON first, otherwise return as string
                if let Ok(json_value) = serde_json::from_str(&content) {
                    Ok(json_value)
                } else {
                    Ok(serde_json::Value::String(content))
                }
            } else {
                Ok(serde_json::Value::Null)
            }
        } else {
            let error_msg = result.error.unwrap_or_else(|| "Tool execution failed".to_string());
            Err(ToolError::ToolCallError(error_msg.into()))
        }
    }
}

/// Extension trait for ToolRegistry to create rig ToolSets
pub trait ToolRegistryRigExt {
    /// Convert this registry to a rig ToolSet
    fn to_rig_toolset(&self) -> ToolSet;
}

impl ToolRegistryRigExt for crate::tools::ToolRegistry {
    fn to_rig_toolset(&self) -> ToolSet {
        let adapters: Vec<RigToolAdapter> = self.names()
            .into_iter()
            .filter_map(|name| self.get(name))
            .map(|tool| RigToolAdapter::new(tool))
            .collect();

        ToolSet::from_tools(adapters)
    }
}
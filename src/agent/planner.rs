//! Planning phase - generates structured execution plans
//!
//! This module handles the planning phase of the Planner-Worker-Reflector pattern.
//! It prompts the LLM to create a structured JSON execution plan based on the
//! user's query and available tools.

use serde_json;
use crate::agent::prompts::PromptTemplate;
use crate::agent::rig_client::RigLlmClient;
use crate::agent::types::*;
use crate::error::{Error, Result};
use crate::tools::ToolRegistry;

/// Planner for generating execution plans
pub struct Planner<'a> {
    /// Rig LLM client
    client: &'a RigLlmClient,
    /// Tool registry for getting tool descriptions
    tools: &'a ToolRegistry,
    /// Model to use for planning
    model: &'a str,
}

impl<'a> Planner<'a> {
    /// Create a new planner
    pub fn new(client: &'a RigLlmClient, tools: &'a ToolRegistry, model: &'a str) -> Self {
        Self { client, tools, model }
    }

    /// Generate an execution plan for the given messages
    pub async fn plan(&self, messages: &[Message]) -> Result<ExecutionPlan> {
        // Get the last user message as the goal
        let goal = self.extract_goal(messages)?;

        // Get tool descriptions for the prompt
        let tool_descriptions = self.get_tool_descriptions();

        // Create planning prompt
        let prompt = PromptTemplate::planner_prompt(&goal, &tool_descriptions);

        // Call LLM with precise settings (temperature 0.0)
        let response = self.client.complete(self.model, prompt).await?;

        // Parse the JSON response
        self.parse_plan_response(&response, &goal)
    }

    /// Extract the user's goal from the conversation
    fn extract_goal(&self, messages: &[Message]) -> Result<String> {
        // Find the last user message
        for message in messages.iter().rev() {
            if message.role == Role::User {
                return Ok(message.content.clone());
            }
        }
        Err(Error::Provider("No user message found in conversation".to_string()))
    }

    /// Get descriptions of all available tools
    fn get_tool_descriptions(&self) -> Vec<String> {
        self.tools.names()
            .into_iter()
            .filter_map(|name| self.tools.get(name))
            .map(|tool| {
                format!("- {}: {}", tool.name(), tool.description())
            })
            .collect()
    }

    /// Parse the LLM response into an ExecutionPlan
    fn parse_plan_response(&self, response: &str, goal: &str) -> Result<ExecutionPlan> {
        // Try to parse as JSON first
        if let Ok(plan) = serde_json::from_str::<ExecutionPlan>(response) {
            return Ok(plan);
        }

        // If JSON parsing fails, check if it's a direct answer (no tools needed)
        if !response.trim().is_empty() && !self.contains_tool_calls(response) {
            return Ok(ExecutionPlan {
                goal: goal.to_string(),
                steps: vec![],
                reasoning: "Direct answer - no tools needed".to_string(),
            });
        }

        // If we can't parse it, return an error
        Err(Error::Provider(format!("Failed to parse execution plan from LLM response: {}", response)))
    }

    /// Check if the response contains tool calls
    fn contains_tool_calls(&self, response: &str) -> bool {
        let response_lower = response.to_lowercase();
        self.tools.names().iter().any(|tool_name| {
            response_lower.contains(&tool_name.to_lowercase())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolRegistry;

    #[tokio::test]
    async fn test_planner_creation() {
        // This is a basic test - full integration tests will come later
        let tools = ToolRegistry::new();
        let client = RigLlmClient::new(crate::config::OpenRouterConfig {
            api_key: secrecy::SecretString::new("test".into()),
            default_model: "test".to_string(),
            site_url: None,
            site_name: None,
            base_url: "https://test.com".to_string(),
            timeout_secs: 30,
            max_retries: 3,
        }).unwrap();
        let planner = Planner::new(&client, &tools, "test-model");

        assert_eq!(planner.model, "test-model");
    }
}
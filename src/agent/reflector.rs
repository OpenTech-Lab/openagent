//! Reflection phase - reviews execution results and decides next steps
//!
//! This module handles the reflection phase of the Planner-Worker-Reflector pattern.
//! It prompts the LLM to review the execution results and decide whether the goal
//! has been achieved or if replanning is needed.

use crate::agent::prompts::PromptTemplate;
use crate::agent::rig_client::RigLlmClient;
use crate::agent::types::*;
use crate::error::Result;

/// Reflector for reviewing execution results
pub struct Reflector<'a> {
    /// Rig LLM client
    client: &'a RigLlmClient,
    /// Model to use for reflection
    model: &'a str,
    /// Maximum number of replans allowed
    max_replans: u32,
}

impl<'a> Reflector<'a> {
    /// Create a new reflector
    pub fn new(client: &'a RigLlmClient, model: &'a str, max_replans: u32) -> Self {
        Self { client, model, max_replans }
    }

    /// Review execution results and decide next action
    pub async fn reflect(
        &self,
        plan: &ExecutionPlan,
        results: &[StepResult],
        attempt: u32,
    ) -> Result<ReflectionDecision> {
        // Format execution results for the LLM
        let results_summary = self.format_results(results);

        // Create reflection prompt
        let prompt = if attempt >= self.max_replans {
            // Max replans reached - force completion
            PromptTemplate::final_reflection_prompt(&plan.goal, &results_summary)
        } else {
            PromptTemplate::reflection_prompt(&plan.goal, &results_summary, attempt)
        };

        // Call LLM with balanced settings (temperature 0.5)
        let response = self.client.complete(self.model, prompt).await?;

        // Parse the reflection response
        self.parse_reflection_response(&response, attempt)
    }

    /// Format execution results into a readable summary
    fn format_results(&self, results: &[StepResult]) -> String {
        if results.is_empty() {
            return "No steps were executed.".to_string();
        }

        let mut summary = String::new();
        summary.push_str("Execution Results:\n");

        for result in results {
            summary.push_str(&format!(
                "Step {}: {} - {}\n",
                result.step_index,
                if result.success { "SUCCESS" } else { "FAILED" },
                result.content.as_deref().unwrap_or_else(|| result.error.as_deref().unwrap_or("No output"))
            ));
        }

        summary
    }

    /// Parse the LLM's reflection response
    fn parse_reflection_response(&self, response: &str, attempt: u32) -> Result<ReflectionDecision> {
        let response_lower = response.to_lowercase();

        // Check for replan indicators
        if response_lower.contains("replan") ||
           response_lower.contains("try again") ||
           response_lower.contains("different approach") ||
           response.starts_with("REPLAN:") {
            return Ok(ReflectionDecision::Replan);
        }

        // Check for completion indicators
        if response_lower.contains("complete") ||
           response_lower.contains("achieved") ||
           response_lower.contains("finished") ||
           response_lower.contains("goal reached") {
            return Ok(ReflectionDecision::Complete(response.to_string()));
        }

        // If no clear decision, try to extract a final answer
        if !response.trim().is_empty() {
            return Ok(ReflectionDecision::Complete(response.to_string()));
        }

        // Default to replan if unclear
        Ok(ReflectionDecision::Replan)
    }
}

/// Decision from the reflection phase
#[derive(Debug, Clone)]
pub enum ReflectionDecision {
    /// Goal achieved - provide final answer
    Complete(String),
    /// Need to replan and try again
    Replan,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_results_empty() {
        let client = RigLlmClient::new(crate::config::OpenRouterConfig {
            api_key: secrecy::SecretString::new("test".into()),
            default_model: "test".to_string(),
            site_url: None,
            site_name: None,
            base_url: "https://test.com".to_string(),
            timeout_secs: 30,
            max_retries: 3,
        }).unwrap();
        let reflector = Reflector::new(&client, "test-model", 3);

        let results = vec![];
        let summary = reflector.format_results(&results);
        assert_eq!(summary, "No steps were executed.");
    }

    #[test]
    fn test_format_results_with_data() {
        let client = RigLlmClient::new(crate::config::OpenRouterConfig {
            api_key: secrecy::SecretString::new("test".into()),
            default_model: "test".to_string(),
            site_url: None,
            site_name: None,
            base_url: "https://test.com".to_string(),
            timeout_secs: 30,
            max_retries: 3,
        }).unwrap();
        let reflector = Reflector::new(&client, "test-model", 3);

        let results = vec![
            StepResult::success(0, "Found information".to_string(), 100),
            StepResult::failure(1, "Tool failed".to_string(), 50),
        ];
        let summary = reflector.format_results(&results);
        assert!(summary.contains("SUCCESS"));
        assert!(summary.contains("FAILED"));
    }
}
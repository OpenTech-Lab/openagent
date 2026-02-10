//! Planner-Worker-Reflector State Machine
//!
//! This module implements the core state machine for the Planner-Worker-Reflector
//! pattern, replacing the simple ReAct loop with a more structured reasoning approach.

use std::sync::Arc;
use tokio::time::Instant;

use crate::agent::types::*;
use crate::agent::agentic_loop::{AgentLoopInput, AgentLoopOutput, LoopCallback, LoopConfig, LoopTrace};
use crate::agent::rig_client::RigLlmClient;
use crate::error::{Error, Result};
use crate::tools::ToolRegistry;

/// Core Planner-Worker-Reflector state machine
pub struct PlannerWorkerReflector {
    /// Rig LLM client for completions
    rig_client: Arc<RigLlmClient>,
    /// Legacy client for fallback (can be removed later)
    legacy_client: Arc<crate::agent::client::OpenRouterClient>,
    /// Tool registry
    tools: Arc<ToolRegistry>,
    /// Loop configuration
    config: LoopConfig,
}

impl PlannerWorkerReflector {
    /// Create a new Planner-Worker-Reflector instance
    pub fn new(
        rig_client: Arc<RigLlmClient>,
        legacy_client: Arc<crate::agent::client::OpenRouterClient>,
        tools: Arc<ToolRegistry>,
        config: LoopConfig,
    ) -> Self {
        Self {
            rig_client,
            legacy_client,
            tools,
            config,
        }
    }

    /// Run the state machine with the given messages
    pub async fn run<'a, C: LoopCallback>(
        &mut self,
        input: &'a AgentLoopInput<'a, C>,
    ) -> Result<AgentLoopOutput> {
        let mut state = AgentState::Planning {
            messages: input.messages.clone(),
            attempt: 0,
        };

        let start_time = Instant::now();

        // Notify callback of loop start
        input.callback.on_iteration_start(0).await;

        loop {
            match state {
                AgentState::Planning { messages, attempt } => {
                    state = self.handle_planning(messages, attempt).await?;
                }
                AgentState::Executing { plan, step_index, results, messages } => {
                    state = self.handle_executing(plan, step_index, results, messages).await?;
                }
                AgentState::Reflecting { plan, results, messages } => {
                    state = self.handle_reflecting(plan, results, messages).await?;
                }
                AgentState::Complete { response, trace, messages, usage } => {
                    // Notify callback of completion
                    input.callback.on_loop_complete(&trace).await;

                    return Ok(AgentLoopOutput {
                        response,
                        trace,
                        final_messages: messages,
                        total_usage: usage.unwrap_or_else(|| crate::agent::types::Usage {
                            prompt_tokens: 0,
                            completion_tokens: 0,
                            total_tokens: 0,
                        }),
                    });
                }
                AgentState::Failed { error, trace } => {
                    // Notify callback of failure
                    input.callback.on_loop_complete(&trace).await;

                    return Err(Error::Provider(error));
                }
            }
        }
    }

    /// Handle the planning state
    async fn handle_planning(&self, messages: Vec<Message>, attempt: u32) -> Result<AgentState> {
        // TODO: Implement planner logic
        // For now, return a simple plan that just answers directly
        let plan = ExecutionPlan {
            goal: "Answer the user's query".to_string(),
            steps: vec![],
            reasoning: "No tools needed for this query".to_string(),
        };

        Ok(AgentState::Reflecting {
            plan,
            results: vec![],
            messages,
        })
    }

    /// Handle the executing state
    async fn handle_executing(
        &self,
        plan: ExecutionPlan,
        step_index: usize,
        results: Vec<StepResult>,
        messages: Vec<Message>,
    ) -> Result<AgentState> {
        // TODO: Implement worker logic
        // For now, just move to reflecting
        Ok(AgentState::Reflecting {
            plan,
            results,
            messages,
        })
    }

    /// Handle the reflecting state
    async fn handle_reflecting(
        &self,
        plan: ExecutionPlan,
        results: Vec<StepResult>,
        messages: Vec<Message>,
    ) -> Result<AgentState> {
        // TODO: Implement reflector logic
        // For now, just complete with a simple response
        let response = "This is a placeholder response from the state machine.".to_string();

        let trace = LoopTrace {
            steps: vec![],
            outcome: crate::agent::agentic_loop::LoopOutcome::Completed,
            total_duration_ms: 100,
        };

        Ok(AgentState::Complete {
            response,
            trace,
            messages,
            usage: None,
        })
    }
}
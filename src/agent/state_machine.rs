//! Planner-Worker-Reflector State Machine
//!
//! This module implements the core state machine for the Planner-Worker-Reflector
//! pattern, replacing the simple ReAct loop with a more structured reasoning approach.

use std::sync::Arc;
use tokio::time::Instant;

use crate::agent::types::*;
use crate::agent::agentic_loop::{AgentLoopInput, AgentLoopOutput, LoopCallback, LoopConfig, LoopTrace};
use crate::agent::rig_client::RigLlmClient;
use crate::agent::planner::Planner;
use crate::agent::reflector::{Reflector, ReflectionDecision};
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
        // Create planner with rig client and tools
        let model = self.rig_client.default_model();
        let planner = Planner::new(&self.rig_client, &self.tools, model);

        // Generate execution plan
        let plan = planner.plan(&messages).await?;

        // Check if we have steps to execute
        if plan.steps.is_empty() {
            // No steps needed - go directly to reflecting
            Ok(AgentState::Reflecting {
                plan,
                results: vec![],
                messages,
            })
        } else {
            // We have steps - go to executing
            Ok(AgentState::Executing {
                plan,
                step_index: 0,
                results: vec![],
                messages,
            })
        }
    }

    /// Handle the executing state
    async fn handle_executing(
        &self,
        plan: ExecutionPlan,
        step_index: usize,
        mut results: Vec<StepResult>,
        messages: Vec<Message>,
    ) -> Result<AgentState> {
        // Get the current step
        let step = match plan.steps.get(step_index) {
            Some(s) => s,
            None => {
                // No more steps - move to reflecting
                return Ok(AgentState::Reflecting {
                    plan,
                    results,
                    messages,
                });
            }
        };

        // Execute the tool
        let start_time = Instant::now();
        let tool_result = match self.tools.get(&step.tool_name) {
            Some(tool) => tool.execute(step.args.clone()).await,
            None => {
                // Tool not found
                let error = format!("Tool '{}' not found", step.tool_name);
                Ok(crate::tools::ToolResult {
                    success: false,
                    content: None,
                    error: Some(error.clone()),
                })
            }
        };

        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Record the result
        let step_result = match tool_result {
            Ok(result) => {
                if result.success {
                    StepResult::success(
                        step_index,
                        result.content.unwrap_or_else(|| "Tool succeeded with no output".to_string()),
                        duration_ms,
                    )
                } else {
                    StepResult::failure(
                        step_index,
                        result.error.unwrap_or_else(|| "Tool failed with no error message".to_string()),
                        duration_ms,
                    )
                }
            }
            Err(e) => StepResult::failure(step_index, format!("Tool execution error: {}", e), duration_ms),
        };

        results.push(step_result);

        // Move to next step or reflecting
        let next_index = step_index + 1;
        if next_index < plan.steps.len() {
            // More steps to execute
            Ok(AgentState::Executing {
                plan,
                step_index: next_index,
                results,
                messages,
            })
        } else {
            // All steps executed - move to reflecting
            Ok(AgentState::Reflecting {
                plan,
                results,
                messages,
            })
        }
    }

    /// Handle the reflecting state
    async fn handle_reflecting(
        &self,
        plan: ExecutionPlan,
        results: Vec<StepResult>,
        messages: Vec<Message>,
    ) -> Result<AgentState> {
        // Create reflector with rig client
        let model = self.rig_client.default_model();
        let max_replans = self.config.max_iterations.unwrap_or(3);
        let reflector = Reflector::new(&self.rig_client, model, max_replans);

        // Count current attempt (number of times we've planned)
        let attempt = results.iter()
            .filter(|r| r.step_index == 0)
            .count() as u32;

        // Reflect on the execution results
        let decision = reflector.reflect(&plan, &results, attempt).await?;

        match decision {
            ReflectionDecision::Complete(final_response) => {
                // Goal achieved - create trace and complete
                let trace = LoopTrace {
                    steps: vec![], // TODO: Convert StepResults to TraceSteps
                    outcome: crate::agent::agentic_loop::LoopOutcome::Completed,
                    total_duration_ms: results.iter().map(|r| r.duration_ms).sum(),
                };

                Ok(AgentState::Complete {
                    response: final_response,
                    trace,
                    messages,
                    usage: None, // TODO: Track usage stats
                })
            }
            ReflectionDecision::Replan => {
                // Need to replan - check if we've exceeded max attempts
                if attempt >= max_replans {
                    // Max replans reached - force completion with summary
                    let summary = format!(
                        "Maximum replanning attempts reached ({}/{}). Last execution results:\n{}",
                        attempt,
                        max_replans,
                        results.iter()
                            .map(|r| format!("Step {}: {}", r.step_index,
                                r.content.as_deref().unwrap_or_else(|| r.error.as_deref().unwrap_or("No output"))))
                            .collect::<Vec<_>>()
                            .join("\n")
                    );

                    let trace = LoopTrace {
                        steps: vec![],
                        outcome: crate::agent::agentic_loop::LoopOutcome::MaxIterationsReached,
                        total_duration_ms: results.iter().map(|r| r.duration_ms).sum(),
                    };

                    Ok(AgentState::Complete {
                        response: summary,
                        trace,
                        messages,
                        usage: None,
                    })
                } else {
                    // Replan with incremented attempt
                    Ok(AgentState::Planning {
                        messages,
                        attempt: attempt + 1,
                    })
                }
            }
        }
    }
}
//! Shared agentic loop engine.
//!
//! Extracts the common LLM-call-tool-call-loop pattern from gateway, TUI,
//! and scheduler into a single reusable function with configurable callbacks,
//! limits, and structured tracing.

use crate::agent::loop_guard::LoopGuard;
use crate::agent::types::*;
use crate::agent::OpenRouterClient;
use crate::error::Result;
use std::sync::Arc;
use crate::tools::{ToolCall, ToolRegistry};

use async_trait::async_trait;
use std::time::Instant;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configurable limits for the agentic loop.
#[derive(Debug, Clone)]
pub struct LoopConfig {
    /// Maximum LLM round-trips before the loop is forcefully stopped.
    pub max_iterations: u32,
    /// Maximum total tool calls across all iterations.
    pub max_tool_calls: u32,
    /// LLM generation options (temperature, max_tokens, etc.).
    pub generation_options: GenerationOptions,
    /// If true, inject a planning system message before the first iteration.
    pub enable_planning_prompt: bool,
    /// If true, inject a reflection system message after each tool-result batch.
    pub enable_reflection_prompt: bool,
    /// Fallback text returned when the loop exits without a final response.
    pub fallback_message: String,
    /// If true, use the new Planner-Worker-Reflector state machine instead of the legacy ReAct loop.
    pub use_state_machine: bool,
}

impl LoopConfig {
    /// Configuration suitable for the Telegram gateway (generous limits).
    pub fn gateway() -> Self {
        Self::gateway_with_state_machine(false)
    }

    /// Configuration suitable for the Telegram gateway with state machine control.
    pub fn gateway_with_state_machine(use_state_machine: bool) -> Self {
        Self {
            max_iterations: 50,
            max_tool_calls: 30,
            generation_options: GenerationOptions::balanced(),
            enable_planning_prompt: false,
            enable_reflection_prompt: false,
            fallback_message: "I searched for information but couldn't find specific results. Please try a more specific query.".into(),
            use_state_machine,
        }
    }

    /// Configuration suitable for the TUI.
    pub fn tui() -> Self {
        Self::tui_with_state_machine(false)
    }

    /// Configuration suitable for the TUI with state machine control.
    pub fn tui_with_state_machine(use_state_machine: bool) -> Self {
        Self {
            max_iterations: 20,
            max_tool_calls: 20,
            generation_options: GenerationOptions::balanced(),
            enable_planning_prompt: false,
            enable_reflection_prompt: false,
            fallback_message: "I reached the maximum number of iterations. Please try a more specific request.".into(),
            use_state_machine,
        }
    }

    /// Configuration suitable for the scheduler.
    pub fn scheduler() -> Self {
        Self::scheduler_with_state_machine(false)
    }

    /// Configuration suitable for the scheduler with state machine control.
    pub fn scheduler_with_state_machine(use_state_machine: bool) -> Self {
        Self {
            max_iterations: 20,
            max_tool_calls: 20,
            generation_options: GenerationOptions::balanced(),
            enable_planning_prompt: false,
            enable_reflection_prompt: false,
            fallback_message: String::new(),
            use_state_machine,
        }
    }
}

// ---------------------------------------------------------------------------
// Structured trace types
// ---------------------------------------------------------------------------

/// A recorded action (tool call) and its observation (result).
#[derive(Debug, Clone)]
pub struct ToolAction {
    pub tool_name: String,
    pub arguments: String,
    pub observation: ToolObservation,
}

/// The result of executing a single tool call.
#[derive(Debug, Clone)]
pub struct ToolObservation {
    pub success: bool,
    pub content: String,
    pub duration_ms: u64,
    pub loop_guard_triggered: bool,
}

/// One iteration of the agentic loop.
#[derive(Debug, Clone)]
pub struct LoopStep {
    pub iteration: u32,
    /// Text content produced by the LLM in this iteration (may be empty).
    pub thought: String,
    /// Tool calls executed in this iteration.
    pub actions: Vec<ToolAction>,
    /// The LLM's finish_reason for this iteration.
    pub finish_reason: String,
    pub timestamp: Instant,
}

/// Full trace of a loop execution.
#[derive(Debug, Clone)]
pub struct LoopTrace {
    pub steps: Vec<LoopStep>,
    pub outcome: LoopOutcome,
    pub total_duration_ms: u64,
}

/// How the loop finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopOutcome {
    /// LLM returned finish_reason "stop" / "end_turn".
    Completed,
    /// Hit `max_iterations` without a stop signal.
    MaxIterationsExceeded,
    /// Hit `max_tool_calls`; final response taken from content-only call.
    ToolLimitReached,
    /// LLM returned an empty response without tool calls.
    EmptyResponse,
    /// LLM API returned an error.
    LlmError(String),
}

// ---------------------------------------------------------------------------
// Callback trait
// ---------------------------------------------------------------------------

/// Trait for callers to hook into loop events (e.g. send typing indicators,
/// print progress to the terminal, etc.).
#[async_trait]
pub trait LoopCallback: Send + Sync {
    /// Called at the start of each iteration, before the LLM call.
    async fn on_iteration_start(&self, _iteration: u32) {}
    /// Called after each individual tool has been executed.
    async fn on_tool_executed(&self, _tool_name: &str, _observation: &ToolObservation) {}
    /// Called at the end of each iteration, after all tool results are collected.
    async fn on_iteration_end(&self, _step: &LoopStep) {}
    /// Called once after the loop terminates.
    async fn on_loop_complete(&self, _trace: &LoopTrace) {}
}

/// Default no-op callback.
pub struct NoOpCallback;
impl NoOpCallback {
    pub fn new() -> Self {
        Self
    }
}
#[async_trait]
impl LoopCallback for NoOpCallback {}

// ---------------------------------------------------------------------------
// Input / Output
// ---------------------------------------------------------------------------

/// Everything the loop needs to run.
pub struct AgentLoopInput<'a, C: LoopCallback> {
    /// The conversation messages (system + user + prior context).
    pub messages: Vec<Message>,
    /// LLM client to call.
    pub llm_client: &'a OpenRouterClient,
    /// Tool registry to execute tools against.
    pub tools: &'a ToolRegistry,
    /// Pre-computed tool definitions (avoids recomputing per-iteration).
    pub tool_definitions: Vec<ToolDefinition>,
    /// Loop configuration.
    pub config: LoopConfig,
    /// User ID — injected into memory/task tool arguments.
    pub user_id: Option<String>,
    /// Chat ID — injected into memory/task tool arguments.
    pub chat_id: Option<i64>,
    /// Event callback.
    pub callback: C,
}

/// The result of running the agentic loop.
pub struct AgentLoopOutput {
    /// The final assistant response text.
    pub response: String,
    /// Structured trace of the full execution.
    pub trace: LoopTrace,
    /// The full messages vector at the end (including tool results etc.).
    pub final_messages: Vec<Message>,
    /// Accumulated token usage across all iterations.
    pub total_usage: Usage,
}

// ---------------------------------------------------------------------------
// Core loop implementation
// ---------------------------------------------------------------------------

/// Unified agent runner that dispatches to either the legacy ReAct loop or the new Planner-Worker-Reflector state machine.
///
/// This function provides a single entry point for running agents, with feature flag control
/// over which implementation to use.
pub async fn run_agent<C: LoopCallback>(
    input: AgentLoopInput<'_, C>,
    rig_client: Option<&std::sync::Arc<crate::agent::rig_client::RigLlmClient>>,
) -> Result<AgentLoopOutput> {
    if input.config.use_state_machine {
        // Use new Planner-Worker-Reflector state machine
        if let Some(rig_client) = rig_client {
            let mut state_machine = crate::agent::state_machine::PlannerWorkerReflector::new(
                rig_client.clone(),
                Arc::new(input.llm_client.clone()),
                Arc::new(input.tools.clone()),
                input.config.clone(),
            );
            state_machine.run(&input).await
        } else {
            // Fallback to legacy if no rig client provided
            run_agentic_loop(input).await
        }
    } else {
        // Use legacy ReAct loop
        run_agentic_loop(input).await
    }
}

/// Run the unified agentic loop.
///
/// Calls the LLM, executes tool calls, feeds results back, and repeats until
/// the LLM stops requesting tools or limits are hit.
#[deprecated(note = "Use run_agent() instead")]
pub async fn run_agentic_loop<C: LoopCallback>(
    input: AgentLoopInput<'_, C>,
) -> Result<AgentLoopOutput> {
    let AgentLoopInput {
        mut messages,
        llm_client,
        tools,
        tool_definitions,
        config,
        user_id,
        chat_id,
        callback,
    } = input;

    let loop_start = Instant::now();

    // Optionally inject planning instructions
    if config.enable_planning_prompt {
        inject_planning_instructions(&mut messages);
    }

    let mut iteration: u32 = 0;
    let mut tool_calls_made: u32 = 0;
    let mut final_response = String::new();
    let mut loop_guard = LoopGuard::default();
    let mut steps: Vec<LoopStep> = Vec::new();
    let mut total_usage = Usage {
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
    };
    // Every loop branch sets `outcome` before breaking; this default is only a
    // safety net in case the loop somehow exits without setting it.
    #[allow(unused_assignments)]
    let mut outcome = LoopOutcome::Completed;

    loop {
        iteration += 1;
        let iter_start = Instant::now();
        info!("Agent loop iteration {}/{}", iteration, config.max_iterations);

        callback.on_iteration_start(iteration).await;

        // Check iteration limit
        if iteration > config.max_iterations {
            warn!("Agent loop exceeded max iterations, using accumulated results");
            if final_response.is_empty() {
                final_response = config.fallback_message.clone();
            }
            outcome = LoopOutcome::MaxIterationsExceeded;
            break;
        }

        // Decide whether to send tool definitions
        let use_tools = tool_calls_made < config.max_tool_calls && !tool_definitions.is_empty();

        // Call LLM
        let response = if use_tools {
            llm_client
                .chat_with_tools(
                    messages.clone(),
                    tool_definitions.clone(),
                    config.generation_options.clone(),
                )
                .await
        } else {
            llm_client
                .chat(messages.clone(), config.generation_options.clone())
                .await
        };

        let response = match response {
            Ok(resp) => resp,
            Err(e) => {
                let err_str = e.to_string();
                outcome = LoopOutcome::LlmError(err_str);
                break;
            }
        };

        // Accumulate usage
        if let Some(ref usage) = response.usage {
            accumulate_usage(&mut total_usage, usage);
        }

        // Get the first choice
        let choice = match response.choices.first() {
            Some(c) => c,
            None => {
                outcome = LoopOutcome::EmptyResponse;
                if final_response.is_empty() {
                    final_response = config.fallback_message.clone();
                }
                break;
            }
        };

        let finish_reason = choice
            .finish_reason
            .as_deref()
            .unwrap_or("unknown")
            .to_string();

        info!(
            "LLM finish_reason: {}, has_content: {}, has_tool_calls: {}",
            finish_reason,
            !choice.message.content.is_empty(),
            choice.message.tool_calls.is_some()
        );

        // --- "stop" / "end_turn" → final response -------------------------
        if finish_reason == "stop" || finish_reason == "end_turn" {
            final_response = choice.message.content.clone();
            info!("LLM finished with reason: {}", finish_reason);
            if !final_response.is_empty() {
                debug!(
                    "Agent reply: {}",
                    &final_response[..final_response.len().min(500)]
                );
            }

            let step = LoopStep {
                iteration,
                thought: final_response.clone(),
                actions: vec![],
                finish_reason: finish_reason.clone(),
                timestamp: iter_start,
            };
            callback.on_iteration_end(&step).await;
            steps.push(step);
            outcome = LoopOutcome::Completed;
            break;
        }

        // --- Tool calls ----------------------------------------------------
        if use_tools {
            if let Some(tool_calls_list) = &choice.message.tool_calls {
                if !tool_calls_list.is_empty() {
                    info!(
                        "LLM requested {} tool calls (total so far: {})",
                        tool_calls_list.len(),
                        tool_calls_made
                    );

                    // Add the assistant message (with tool_calls) to context
                    messages.push(choice.message.clone());

                    let mut actions = Vec::new();

                    for tc in tool_calls_list.iter() {
                        tool_calls_made += 1;

                        let tool_name = &tc.function.name;

                        // Parse arguments
                        let args: serde_json::Value =
                            match serde_json::from_str(&tc.function.arguments) {
                                Ok(v) => v,
                                Err(e) => {
                                    warn!(
                                        "Failed to parse tool arguments for {}: {}",
                                        tool_name, e
                                    );
                                    serde_json::json!({})
                                }
                            };

                        info!(
                            "Executing tool: {} (call #{}/{})",
                            tool_name, tool_calls_made, config.max_tool_calls
                        );
                        debug!("Tool {} arguments: {}", tool_name, tc.function.arguments);

                        // Inject _user_id / _chat_id for memory and task tools
                        let call_args = inject_user_context(args, &user_id, &chat_id, tool_name);

                        let call = ToolCall {
                            id: tc.id.clone(),
                            name: tool_name.clone(),
                            arguments: call_args,
                        };

                        let tool_start = Instant::now();
                        let result = tools.execute(&call).await;
                        let duration_ms = tool_start.elapsed().as_millis() as u64;

                        let result_content = match result {
                            Ok(r) => {
                                let s = r.to_string();
                                info!(
                                    "Tool {} succeeded, result length: {} chars",
                                    tool_name,
                                    s.len()
                                );
                                debug!(
                                    "Tool {} result: {}",
                                    tool_name,
                                    &s[..s.len().min(1000)]
                                );
                                s
                            }
                            Err(e) => {
                                let err = format!("Tool error: {}", e);
                                warn!("Tool {} failed: {}", tool_name, err);
                                err
                            }
                        };

                        // Add tool result to messages
                        messages.push(Message::tool(&tc.id, &result_content));

                        // Check for stuck loops
                        let loop_guard_triggered = if let Some(hint) = loop_guard.record(
                            tool_name,
                            &tc.function.arguments,
                            &result_content,
                        ) {
                            warn!(
                                "Loop guard triggered for tool '{}', injecting hint",
                                tool_name
                            );
                            messages.push(Message::user(&hint));
                            true
                        } else {
                            false
                        };

                        let observation = ToolObservation {
                            success: !result_content.starts_with("Tool error:")
                                && !result_content.starts_with("Error:"),
                            content: result_content,
                            duration_ms,
                            loop_guard_triggered,
                        };

                        callback.on_tool_executed(tool_name, &observation).await;

                        actions.push(ToolAction {
                            tool_name: tool_name.clone(),
                            arguments: tc.function.arguments.clone(),
                            observation,
                        });
                    }

                    // Optionally inject reflection prompt after tool results
                    if config.enable_reflection_prompt {
                        inject_reflection_prompt(&mut messages);
                    }

                    let step = LoopStep {
                        iteration,
                        thought: choice.message.content.clone(),
                        actions,
                        finish_reason: finish_reason.clone(),
                        timestamp: iter_start,
                    };
                    callback.on_iteration_end(&step).await;
                    steps.push(step);

                    // Continue loop — LLM will process tool results
                    continue;
                }
            }
        }

        // --- No tool calls: content is final response ----------------------
        if !choice.message.content.is_empty() {
            final_response = choice.message.content.clone();
            info!("LLM returned content without tool calls, treating as final");
            if !final_response.is_empty() {
                debug!(
                    "Agent reply: {}",
                    &final_response[..final_response.len().min(500)]
                );
            }

            let step = LoopStep {
                iteration,
                thought: final_response.clone(),
                actions: vec![],
                finish_reason: finish_reason.clone(),
                timestamp: iter_start,
            };
            callback.on_iteration_end(&step).await;
            steps.push(step);

            if !use_tools && tool_calls_made >= config.max_tool_calls {
                outcome = LoopOutcome::ToolLimitReached;
            } else {
                outcome = LoopOutcome::Completed;
            }
            break;
        }

        // --- Edge case: no content, no tool calls --------------------------
        warn!("LLM returned empty response, finish_reason: {}", finish_reason);
        final_response = config.fallback_message.clone();

        let step = LoopStep {
            iteration,
            thought: String::new(),
            actions: vec![],
            finish_reason: finish_reason.clone(),
            timestamp: iter_start,
        };
        callback.on_iteration_end(&step).await;
        steps.push(step);
        outcome = LoopOutcome::EmptyResponse;
        break;
    }

    let total_duration_ms = loop_start.elapsed().as_millis() as u64;

    let trace = LoopTrace {
        steps,
        outcome: outcome.clone(),
        total_duration_ms,
    };

    callback.on_loop_complete(&trace).await;

    info!(
        "Agentic loop finished: outcome={:?}, iterations={}, tool_calls={}, duration={}ms",
        outcome,
        iteration.min(config.max_iterations),
        tool_calls_made,
        total_duration_ms,
    );

    Ok(AgentLoopOutput {
        response: final_response,
        trace,
        final_messages: messages,
        total_usage,
    })
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Inject a planning system message before the loop starts.
fn inject_planning_instructions(messages: &mut Vec<Message>) {
    let planning_prompt = "\
Before executing any tools, briefly plan your approach: \
What is the user asking for? What information or actions do you need? \
In what order should you proceed?";
    messages.push(Message::system(planning_prompt));
}

/// Inject a reflection system message after a batch of tool results.
fn inject_reflection_prompt(messages: &mut Vec<Message>) {
    let reflection_prompt = "\
Review the results above. Have you gathered enough information to answer the \
user's question completely? If yes, provide your final response. If not, \
explain what's still needed and continue.";
    messages.push(Message::system(reflection_prompt));
}

/// Inject `_user_id` and `_chat_id` into tool arguments for memory/task tools.
fn inject_user_context(
    mut args: serde_json::Value,
    user_id: &Option<String>,
    chat_id: &Option<i64>,
    tool_name: &str,
) -> serde_json::Value {
    if tool_name.starts_with("memory_") || tool_name.starts_with("task_") {
        if let Some(obj) = args.as_object_mut() {
            if let Some(ref uid) = user_id {
                obj.insert("_user_id".to_string(), serde_json::json!(uid));
            }
            if let Some(cid) = chat_id {
                obj.insert("_chat_id".to_string(), serde_json::json!(cid));
            }
        }
    }
    args
}

/// Sum token usage from one response into an accumulator.
fn accumulate_usage(total: &mut Usage, delta: &Usage) {
    total.prompt_tokens += delta.prompt_tokens;
    total.completion_tokens += delta.completion_tokens;
    total.total_tokens += delta.total_tokens;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loop_config_named_constructors() {
        let gw = LoopConfig::gateway();
        assert_eq!(gw.max_iterations, 50);
        assert_eq!(gw.max_tool_calls, 30);

        let tui = LoopConfig::tui();
        assert_eq!(tui.max_iterations, 20);
        assert_eq!(tui.max_tool_calls, 20);

        let sched = LoopConfig::scheduler();
        assert_eq!(sched.max_iterations, 20);
        assert_eq!(sched.max_tool_calls, 20);
    }

    #[test]
    fn test_inject_user_context_memory_tool() {
        let args = serde_json::json!({"query": "hello"});
        let result = inject_user_context(
            args,
            &Some("user-123".into()),
            &Some(456),
            "memory_search",
        );
        assert_eq!(result["_user_id"], "user-123");
        assert_eq!(result["_chat_id"], 456);
    }

    #[test]
    fn test_inject_user_context_non_memory_tool() {
        let args = serde_json::json!({"path": "/tmp/file"});
        let result = inject_user_context(
            args.clone(),
            &Some("user-123".into()),
            &Some(456),
            "read_file",
        );
        // Should NOT have _user_id injected
        assert!(result.get("_user_id").is_none());
    }

    #[test]
    fn test_accumulate_usage() {
        let mut total = Usage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
        };
        let delta = Usage {
            prompt_tokens: 20,
            completion_tokens: 10,
            total_tokens: 30,
        };
        accumulate_usage(&mut total, &delta);
        assert_eq!(total.prompt_tokens, 30);
        assert_eq!(total.completion_tokens, 15);
        assert_eq!(total.total_tokens, 45);
    }
}

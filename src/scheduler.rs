//! Periodic scheduler for soul evolution, summarization, and task processing
//!
//! Runs on a configurable interval (default 30 minutes) and:
//! 1. Summarizes active conversations, updating the soul's mutable sections
//! 2. Picks up and processes pending tasks if the agent is idle

use crate::agent::{
    ConversationManager, GenerationOptions, LoopGuard, Message as AgentMessage, OpenRouterClient,
    ToolCall, ToolRegistry,
    prompts::DEFAULT_SYSTEM_PROMPT,
};
use crate::database::{AgentStatusStore, ConfigParamStore, MemoryType, SoulStore, TaskStore, AgentTask};
use crate::memory::{ConversationSummarizer, MemoryRetriever};
use crate::error::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// The periodic scheduler that handles summarization and task processing
pub struct Scheduler {
    task_store: TaskStore,
    status_store: AgentStatusStore,
    soul_store: SoulStore,
    config_store: ConfigParamStore,
    llm_client: OpenRouterClient,
    memory_retriever: Option<MemoryRetriever>,
    conversations: Arc<RwLock<ConversationManager>>,
    tools: Arc<ToolRegistry>,
}

impl Scheduler {
    pub fn new(
        task_store: TaskStore,
        status_store: AgentStatusStore,
        soul_store: SoulStore,
        config_store: ConfigParamStore,
        llm_client: OpenRouterClient,
        memory_retriever: Option<MemoryRetriever>,
        conversations: Arc<RwLock<ConversationManager>>,
        tools: Arc<ToolRegistry>,
    ) -> Self {
        Self {
            task_store,
            status_store,
            soul_store,
            config_store,
            llm_client,
            memory_retriever,
            conversations,
            tools,
        }
    }

    /// Main scheduler loop. Reads interval from config_params and ticks periodically.
    pub async fn run(self: Arc<Self>) {
        let interval_minutes = self.get_interval_minutes().await;
        let mut interval = tokio::time::interval(
            tokio::time::Duration::from_secs(interval_minutes * 60),
        );

        info!("Scheduler started, interval: {} minutes", interval_minutes);

        // Skip the first immediate tick
        interval.tick().await;

        loop {
            interval.tick().await;
            info!("Scheduler tick");

            if let Err(e) = self.tick().await {
                error!("Scheduler tick failed: {}", e);
            }
        }
    }

    async fn tick(&self) -> Result<()> {
        // 1. Heartbeat
        self.status_store.heartbeat().await?;

        // 2. Periodic summarization (always runs if enabled)
        if self.is_summarization_enabled().await {
            if let Err(e) = self.run_summarization().await {
                warn!("Summarization failed: {}", e);
            }
        }

        // 3. Task processing (only if agent is ready and enabled)
        if self.is_task_processing_enabled().await {
            if self.status_store.is_ready().await.unwrap_or(false) {
                if let Err(e) = self.process_next_task().await {
                    warn!("Task processing failed: {}", e);
                }
            }
        }

        // 4. Record scheduler run
        self.status_store.record_scheduler_run().await?;

        Ok(())
    }

    async fn get_interval_minutes(&self) -> u64 {
        match self.config_store.get("scheduler", "interval_minutes").await {
            Ok(Some(param)) => param.value.parse().unwrap_or(30),
            _ => 30,
        }
    }

    async fn is_summarization_enabled(&self) -> bool {
        match self.config_store.get("scheduler", "summarization_enabled").await {
            Ok(Some(param)) => param.value != "false",
            _ => true,
        }
    }

    async fn is_task_processing_enabled(&self) -> bool {
        match self.config_store.get("scheduler", "task_processing_enabled").await {
            Ok(Some(param)) => param.value != "false",
            _ => true,
        }
    }

    /// Summarize active conversations and update the soul's mutable sections
    async fn run_summarization(&self) -> Result<()> {
        let conversations = self.conversations.read().await;
        let active_users: Vec<String> = conversations
            .active_users()
            .iter()
            .map(|s| s.to_string())
            .collect();

        // Collect message data under a short lock, then release
        let mut user_messages: Vec<(String, Vec<AgentMessage>)> = Vec::new();
        for user_id in &active_users {
            if let Some(conv) = conversations.get(user_id) {
                if conv.message_count() >= 4 {
                    user_messages.push((user_id.clone(), conv.messages.clone()));
                }
            }
        }
        drop(conversations); // Release the read lock

        if user_messages.is_empty() {
            return Ok(());
        }

        let summarizer = ConversationSummarizer::new(self.llm_client.clone());

        for (user_id, messages) in &user_messages {
            match summarizer.summarize(messages).await {
                Ok(summary) => {
                    info!(
                        "Summarized conversation for user {}: {} facts, {} prefs, {} topics",
                        user_id,
                        summary.key_facts.len(),
                        summary.user_preferences.len(),
                        summary.topics.len(),
                    );

                    // Save episodic memory if retriever is available
                    if let Some(ref retriever) = self.memory_retriever {
                        if !summary.summary.is_empty() {
                            let memory = crate::database::Memory::new(user_id, &summary.summary)
                                .with_summary(&summary.summary)
                                .with_importance(0.6)
                                .with_tags(summary.topics.clone())
                                .with_memory_type(MemoryType::Episodic)
                                .with_source("auto:scheduler_summary");
                            if let Err(e) = retriever.store().save(&memory, None).await {
                                warn!("Failed to save episodic memory for {}: {}", user_id, e);
                            }
                        }
                    }

                    // Update soul's Memory & Learning section with new preferences
                    if !summary.user_preferences.is_empty() {
                        if let Err(e) = self.update_soul_learning(&summary.user_preferences).await {
                            warn!("Failed to update soul learning: {}", e);
                        }
                    }
                }
                Err(e) => warn!("Summarization failed for user {}: {}", user_id, e),
            }
        }

        Ok(())
    }

    /// Update the soul's "Memory & Learning" section with new preferences
    async fn update_soul_learning(&self, preferences: &[String]) -> Result<()> {
        if let Some(section) = self.soul_store.get_section("Memory & Learning").await? {
            let mut content = section.content.clone();

            for pref in preferences {
                if content.contains(pref) {
                    continue; // Skip duplicates
                }

                if content.contains("_None learned yet._") || content.contains("None learned yet") {
                    content = content.replace("_None learned yet._", &format!("- {}", pref));
                    content = content.replace("None learned yet", &format!("- {}", pref));
                } else {
                    // Append after the User Preferences sub-section
                    if let Some(pos) = content.find("### User Preferences") {
                        let after = &content[pos..];
                        let insert_pos = after
                            .find("\n### ")
                            .map(|p| pos + p)
                            .unwrap_or(content.len());
                        content.insert_str(insert_pos, &format!("\n- {}", pref));
                    } else {
                        // Fallback: just append to end
                        content.push_str(&format!("\n- {}", pref));
                    }
                }
            }

            self.soul_store.update_section("Memory & Learning", &content).await?;
            info!("Updated soul learning with {} new preferences", preferences.len());
        }

        Ok(())
    }

    /// Pick up and process the next pending task
    async fn process_next_task(&self) -> Result<()> {
        let task = match self.task_store.next_pending().await? {
            Some(t) => t,
            None => return Ok(()), // No pending tasks
        };

        info!("Processing task: {} ({})", task.title, task.id);

        // Try to transition agent to processing
        if let Err(e) = self.status_store.set_processing(task.id).await {
            warn!("Could not transition to processing: {}", e);
            return Ok(());
        }

        // Mark task as processing
        self.task_store.start_processing(task.id).await?;

        // Execute the task using the agentic loop
        let result = self.execute_task(&task).await;

        match result {
            Ok(output) => {
                let truncated = if output.len() > 2000 {
                    &output[..2000]
                } else {
                    &output
                };
                self.task_store.finish(task.id, Some(truncated)).await?;
                info!("Task {} completed successfully", task.id);
            }
            Err(e) => {
                self.task_store.fail(task.id, &e.to_string()).await?;
                error!("Task {} failed: {}", task.id, e);
            }
        }

        // Return to ready
        self.status_store.set_ready().await?;

        Ok(())
    }

    /// Execute a task using a simplified agentic loop
    async fn execute_task(&self, task: &AgentTask) -> Result<String> {
        let system_prompt = match self.soul_store.render_full_soul().await {
            Ok(soul) => format!("{}\n\n---\n\n## Agent Soul\n\n{}", DEFAULT_SYSTEM_PROMPT, soul),
            Err(_) => DEFAULT_SYSTEM_PROMPT.to_string(),
        };

        let mut messages = vec![
            AgentMessage::system(&system_prompt),
            AgentMessage::user(&task.description),
        ];

        let tool_definitions = self.tools.definitions();
        let mut final_response = String::new();
        let mut loop_guard = LoopGuard::default();

        const MAX_ITERATIONS: u32 = 20;
        for _iteration in 0..MAX_ITERATIONS {
            let response = self
                .llm_client
                .chat_with_tools(
                    messages.clone(),
                    tool_definitions.clone(),
                    GenerationOptions::balanced(),
                )
                .await?;

            let choice = match response.choices.first() {
                Some(c) => c,
                None => {
                    return Err(crate::Error::Internal("No LLM response".into()));
                }
            };

            let finish_reason = choice.finish_reason.as_deref().unwrap_or("unknown");

            if finish_reason == "stop" || finish_reason == "end_turn" {
                final_response = choice.message.content.clone();
                break;
            }

            if let Some(tool_calls) = &choice.message.tool_calls {
                if !tool_calls.is_empty() {
                    messages.push(choice.message.clone());

                    for tc in tool_calls {
                        let args: serde_json::Value =
                            serde_json::from_str(&tc.function.arguments)
                                .unwrap_or(serde_json::json!({}));

                        // Inject _user_id for memory tools
                        let mut call_args = args;
                        if tc.function.name.starts_with("memory_") {
                            if let Some(obj) = call_args.as_object_mut() {
                                obj.insert(
                                    "_user_id".to_string(),
                                    serde_json::json!(task.user_id),
                                );
                            }
                        }

                        let call = ToolCall {
                            id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            arguments: call_args,
                        };

                        let result = self.tools.execute(&call).await;
                        let content = match result {
                            Ok(r) => r.to_string(),
                            Err(e) => format!("Tool error: {}", e),
                        };
                        messages.push(AgentMessage::tool(&tc.id, &content));

                        // Check for stuck loops
                        if let Some(hint) = loop_guard.record(
                            &tc.function.name,
                            &tc.function.arguments,
                            &content,
                        ) {
                            warn!("Loop guard triggered for tool '{}' during task processing", tc.function.name);
                            messages.push(AgentMessage::user(&hint));
                        }
                    }
                    continue;
                }
            }

            // No tool calls — treat content as final
            if !choice.message.content.is_empty() {
                final_response = choice.message.content.clone();
                break;
            }

            // Edge case: empty response
            break;
        }

        Ok(final_response)
    }
}

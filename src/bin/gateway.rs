//! OpenAgent Telegram Gateway
//!
//! The main entry point for the Telegram bot interface.

use openagent::agent::{
    ConversationManager, GenerationOptions, Message as AgentMessage, OpenRouterClient,
    ToolRegistry, ToolCall, ReadFileTool, WriteFileTool, 
    DuckDuckGoSearchTool, BraveSearchTool, PerplexitySearchTool,
    prompts::DEFAULT_SYSTEM_PROMPT,
};
use openagent::config::Config;
use openagent::database::{init_pool, MemoryStore, OpenSearchClient};
use openagent::sandbox::{create_executor, CodeExecutor, ExecutionRequest, Language};
use openagent::{Error, Result};

use secrecy::ExposeSecret;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use teloxide::utils::command::BotCommands;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Bot commands
#[allow(dead_code)]
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Available commands:")]
enum Command {
    #[command(description = "Start the bot")]
    Start,
    #[command(description = "Show help")]
    Help,
    #[command(description = "Clear conversation history")]
    Clear,
    #[command(description = "Show current model")]
    Model,
    #[command(description = "Switch model (e.g., /switch anthropic/claude-3.5-sonnet)")]
    Switch(String),
    #[command(description = "Execute code (e.g., /run python print('hello'))")]
    Run(String),
    #[command(description = "Show status")]
    Status,
}

/// Application state shared across handlers
struct AppState {
    config: Config,
    llm_client: OpenRouterClient,
    conversations: RwLock<ConversationManager>,
    memory_store: Option<MemoryStore>,
    executor: Box<dyn CodeExecutor>,
    tools: ToolRegistry,
}

impl AppState {
    async fn new(config: Config) -> Result<Self> {
        // Get OpenRouter config (required for now)
        let openrouter_config = config.provider.openrouter.clone()
            .ok_or_else(|| Error::Config("OpenRouter not configured. Set OPENROUTER_API_KEY environment variable.".into()))?;
        
        // Initialize OpenRouter client
        let llm_client = OpenRouterClient::new(openrouter_config.clone())?;

        // Initialize conversation manager
        let conversations = ConversationManager::new(&openrouter_config.default_model)
            .with_system_prompt(DEFAULT_SYSTEM_PROMPT);

        // Try to initialize database (optional)
        let memory_store = match &config.storage.postgres {
            Some(db_config) => match init_pool(db_config).await {
                Ok(pool) => {
                    // Try to initialize OpenSearch
                    let opensearch = match &config.storage.opensearch {
                        Some(os_config) => match OpenSearchClient::new(os_config).await {
                            Ok(os) => Some(os),
                            Err(e) => {
                                warn!("OpenSearch not available: {}. Using PostgreSQL only.", e);
                                None
                            }
                        },
                        None => None,
                    };
                    Some(MemoryStore::new(pool, opensearch))
                }
                Err(e) => {
                    warn!("Database not available: {}. Running without persistence.", e);
                    None
                }
            },
            None => {
                warn!("Database not configured. Running without persistence.");
                None
            }
        };

        // Initialize code executor
        let executor = create_executor(&config.sandbox).await?;

        // Initialize tool registry
        let mut tools = ToolRegistry::new();
        tools.register(ReadFileTool::new(config.sandbox.allowed_dir.clone()));
        tools.register(WriteFileTool::new(config.sandbox.allowed_dir.clone()));
        
        // Register web search tools (DuckDuckGo is always available, no API key needed)
        tools.register(DuckDuckGoSearchTool::new());
        
        // Register Brave if API key available
        if let Some(brave) = BraveSearchTool::from_env() {
            info!("Brave Search enabled");
            tools.register(brave);
        }
        
        // Register Perplexity if API key available
        if let Some(perplexity) = PerplexitySearchTool::from_env() {
            info!("Perplexity Search enabled");
            tools.register(perplexity);
        }

        Ok(AppState {
            config,
            llm_client,
            conversations: RwLock::new(conversations),
            memory_store,
            executor,
            tools,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("openagent=debug".parse().unwrap())
                .add_directive("teloxide=info".parse().unwrap()),
        )
        .init();

    info!("Starting OpenAgent Gateway v{}", openagent::VERSION);

    // Load configuration
    let config = Config::from_env()?;

    // Get telegram config (optional)
    let telegram_config = match config.channels.telegram.as_ref() {
        Some(cfg) if !cfg.bot_token.expose_secret().is_empty() => Some(cfg),
        Some(_) => {
            warn!("TELEGRAM_BOT_TOKEN is empty, Telegram bot will not start");
            None
        }
        None => {
            warn!("Telegram not configured, Telegram bot will not start");
            warn!("Set TELEGRAM_BOT_TOKEN environment variable to enable Telegram");
            None
        }
    };

    // Initialize application state
    let state = Arc::new(AppState::new(config.clone()).await?);

    let default_model = config.provider.openrouter.as_ref()
        .map(|o| o.default_model.as_str())
        .unwrap_or("not configured");
    info!(
        "Initialized with model: {}",
        default_model
    );
    info!(
        "Execution environment: {}",
        config.sandbox.execution_env
    );

    // Start Telegram bot if configured
    if let Some(telegram_config) = telegram_config {
        // Create bot
        let bot = Bot::new(telegram_config.bot_token.expose_secret());

        // Get bot info
        let me = bot.get_me().await.map_err(|e| Error::Telegram(e.to_string()))?;
        info!("Bot started: @{}", me.username.as_deref().unwrap_or("unknown"));

        // Start dispatcher
        let handler = dptree::entry()
            .branch(Update::filter_message().endpoint(message_handler));

        Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![state])
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;
    } else {
        info!("No channels configured. Gateway running in standby mode.");
        info!("Configure TELEGRAM_BOT_TOKEN to enable Telegram bot.");
        info!("Press Ctrl+C to exit.");
        
        // Wait for shutdown signal
        tokio::signal::ctrl_c().await.expect("Failed to listen for ctrl-c");
    }

    info!("Gateway shutdown complete");
    Ok(())
}

/// Handle incoming messages
async fn message_handler(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
) -> ResponseResult<()> {
    let user_id = msg.from.as_ref().map(|u| u.id.0.to_string()).unwrap_or_default();
    let chat_id = msg.chat.id;

    // Check if user is allowed
    if let Some(telegram_config) = &state.config.channels.telegram {
        if !telegram_config.allow_from.is_empty() {
            let user_id_num: i64 = user_id.parse().unwrap_or(0);
            if !telegram_config.allow_from.contains(&user_id_num) {
                bot.send_message(chat_id, "You are not authorized to use this bot.")
                    .await?;
                return Ok(());
            }
        }
    }

    // Handle commands
    if let Some(text) = msg.text() {
        let text = text.to_string();
        if text.starts_with('/') {
            return handle_command(bot, msg, state, &text).await;
        }

        // Regular message - chat with LLM
        return handle_chat(bot, msg, state, &text, &user_id).await;
    }

    // Handle documents/files
    if msg.document().is_some() {
        return handle_document(bot, msg, state, &user_id).await;
    }

    Ok(())
}

/// Handle bot commands
async fn handle_command(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
    text: &str,
) -> ResponseResult<()> {
    let chat_id = msg.chat.id;
    let user_id = msg.from.as_ref().map(|u| u.id.0.to_string()).unwrap_or_default();

    // Parse command
    let parts: Vec<&str> = text.splitn(2, ' ').collect();
    let cmd = parts[0].trim_start_matches('/').to_lowercase();
    let args = parts.get(1).map(|s| s.to_string()).unwrap_or_default();

    match cmd.as_str() {
        "start" => {
            bot.send_message(
                chat_id,
                "👋 Welcome to OpenAgent!\n\n\
                I'm your AI assistant powered by OpenRouter. \
                I can help you with coding, answer questions, and execute code.\n\n\
                Use /help to see available commands.",
            )
            .await?;
        }
        "help" => {
            bot.send_message(chat_id, Command::descriptions().to_string())
                .await?;
        }
        "clear" => {
            let mut conversations = state.conversations.write().await;
            conversations.clear_conversation(&user_id);
            bot.send_message(chat_id, "✅ Conversation cleared.")
                .await?;
        }
        "model" => {
            let conversations = state.conversations.read().await;
            let default_model = state.config.provider.openrouter
                .as_ref()
                .map(|c| c.default_model.as_str())
                .unwrap_or("unknown");
            let model = conversations
                .get(&user_id)
                .map(|c| c.model.as_str())
                .unwrap_or(default_model);
            bot.send_message(chat_id, format!("Current model: `{}`", model))
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
        "switch" => {
            if args.is_empty() {
                bot.send_message(
                    chat_id,
                    "Usage: /switch <model>\nExample: /switch anthropic/claude-3.5-sonnet",
                )
                .await?;
            } else {
                let mut conversations = state.conversations.write().await;
                let conv = conversations.get_or_create(&user_id);
                conv.model = args.clone();
                bot.send_message(chat_id, format!("✅ Switched to model: {}", args))
                    .await?;
            }
        }
        "run" => {
            if args.is_empty() {
                bot.send_message(
                    chat_id,
                    "Usage: /run <language> <code>\nExample: /run python print('hello')",
                )
                .await?;
            } else {
                handle_code_execution(bot, chat_id, state, &args).await?;
            }
        }
        "status" => {
            let default_model = state.config.provider.openrouter
                .as_ref()
                .map(|c| c.default_model.as_str())
                .unwrap_or("not configured");
            let status = format!(
                "🤖 *OpenAgent Status*\n\n\
                Version: {}\n\
                Model: {}\n\
                Execution: {}\n\
                Database: {}\n\
                Tools: {}",
                openagent::VERSION,
                default_model,
                state.config.sandbox.execution_env,
                if state.memory_store.is_some() { "Connected" } else { "Not connected" },
                state.tools.count()
            );
            bot.send_message(chat_id, status)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
        _ => {
            bot.send_message(chat_id, "Unknown command. Use /help to see available commands.")
                .await?;
        }
    }

    Ok(())
}

/// Handle regular chat messages - AGENTIC LOOP
async fn handle_chat(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
    text: &str,
    user_id: &str,
) -> ResponseResult<()> {
    let chat_id = msg.chat.id;

    // Show typing indicator
    bot.send_chat_action(chat_id, teloxide::types::ChatAction::Typing)
        .await?;

    // Get or create conversation and add user message
    let mut messages = {
        let mut conversations = state.conversations.write().await;
        let conv = conversations.get_or_create(user_id);
        conv.add_user_message(text);
        conv.get_api_messages()
    };

    // Get tool definitions for the LLM
    let tool_definitions = state.tools.definitions();
    
    info!("Starting agent loop with {} tools available", tool_definitions.len());
    
    // Maximum iterations to prevent infinite loops
    const MAX_ITERATIONS: u32 = 3;
    let mut iteration = 0;
    let mut final_response = String::new();
    let mut tool_calls_made = 0u32;
    const MAX_TOOL_CALLS: u32 = 3;

    // Agentic loop: keep running until LLM stops calling tools
    loop {
        iteration += 1;
        info!("Agent loop iteration {}/{}", iteration, MAX_ITERATIONS);
        
        if iteration > MAX_ITERATIONS {
            warn!("Agent loop exceeded max iterations, using accumulated results");
            // Synthesize a response from what we have
            if final_response.is_empty() {
                final_response = "I searched for information but couldn't find specific results. Please try a more specific query.".to_string();
            }
            break;
        }

        // If we've made too many tool calls, stop accepting more
        let use_tools = tool_calls_made < MAX_TOOL_CALLS;
        
        // Call LLM with or without tools based on limits
        let response = if use_tools {
            state.llm_client
                .chat_with_tools(messages.clone(), tool_definitions.clone(), GenerationOptions::balanced())
                .await
        } else {
            // Force no tools - just get a response
            state.llm_client
                .chat(messages.clone(), GenerationOptions::balanced())
                .await
        };
        
        let response = match response {
            Ok(resp) => resp,
            Err(e) => {
                error!("LLM error: {}", e);
                bot.send_message(chat_id, format!("❌ Error: {}", e))
                    .await?;
                return Ok(());
            }
        };

        // Get the first choice
        let choice = match response.choices.first() {
            Some(c) => c,
            None => {
                bot.send_message(chat_id, "❌ No response from LLM")
                    .await?;
                return Ok(());
            }
        };

        let finish_reason = choice.finish_reason.as_deref().unwrap_or("unknown");
        info!("LLM finish_reason: {}, has_content: {}, has_tool_calls: {}", 
            finish_reason,
            !choice.message.content.is_empty(),
            choice.message.tool_calls.is_some()
        );

        // Check finish reason - if "stop" or "end_turn", we're done
        if finish_reason == "stop" || finish_reason == "end_turn" {
            final_response = choice.message.content.clone();
            info!("LLM finished with reason: {}", finish_reason);
            
            // Store assistant response
            {
                let mut conversations = state.conversations.write().await;
                if let Some(conv) = conversations.get_mut(user_id) {
                    conv.add_assistant_message(&final_response);
                    if let Some(usage) = &response.usage {
                        conv.total_tokens += usage.total_tokens;
                    }
                }
            }
            break;
        }

        // Check if there are tool calls (and we haven't hit the limit)
        if use_tools {
            if let Some(tool_calls) = &choice.message.tool_calls {
                if !tool_calls.is_empty() {
                    info!("LLM requested {} tool calls (total so far: {})", tool_calls.len(), tool_calls_made);
                    
                    // Add the assistant message with tool calls to context
                    messages.push(choice.message.clone());

                    // Execute each tool call (limit to first 2 per iteration)
                    for tool_call in tool_calls.iter().take(2) {
                        tool_calls_made += 1;
                        
                        let tool_name = &tool_call.function.name;
                        
                        // Parse arguments
                        let args: serde_json::Value = match serde_json::from_str(&tool_call.function.arguments) {
                            Ok(v) => v,
                            Err(e) => {
                                warn!("Failed to parse tool arguments for {}: {}", tool_name, e);
                                serde_json::json!({})
                            }
                        };

                        info!("Executing tool: {} (call #{}/{})", tool_name, tool_calls_made, MAX_TOOL_CALLS);
                        
                        // Show typing while executing tool
                        let _ = bot.send_chat_action(chat_id, teloxide::types::ChatAction::Typing).await;

                        // Execute the tool
                        let call = ToolCall {
                            id: tool_call.id.clone(),
                            name: tool_name.clone(),
                            arguments: args,
                        };
                        
                        let result = state.tools.execute(&call).await;
                        
                        let result_content = match result {
                            Ok(r) => {
                                let s = r.to_string();
                                info!("Tool {} succeeded, result length: {} chars", tool_name, s.len());
                                s
                            },
                            Err(e) => {
                                let err = format!("Tool error: {}", e);
                                warn!("Tool {} failed: {}", tool_name, err);
                                err
                            }
                        };

                        // Add tool result to messages
                        messages.push(AgentMessage::tool(&tool_call.id, &result_content));
                    }

                    // Continue loop - LLM will process tool results
                    continue;
                }
            }
        }

        // No tool calls (or tools disabled) - treat content as final response
        if !choice.message.content.is_empty() {
            final_response = choice.message.content.clone();
            info!("LLM returned content without tool calls, treating as final");
            
            // Store assistant response in conversation
            {
                let mut conversations = state.conversations.write().await;
                if let Some(conv) = conversations.get_mut(user_id) {
                    conv.add_assistant_message(&final_response);
                    if let Some(usage) = &response.usage {
                        conv.total_tokens += usage.total_tokens;
                    }
                }
            }
            break;
        }
        
        // Edge case: no content, no tool calls - this shouldn't happen often
        warn!("LLM returned empty response, finish_reason: {}", finish_reason);
        final_response = "I'm having trouble processing this request. Please try again.".to_string();
        break;
    }

    // Send response (split if too long)
    if !final_response.is_empty() {
        send_long_message(&bot, chat_id, &final_response).await?;
    }

    Ok(())
}

/// Handle code execution command
async fn handle_code_execution(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    args: &str,
) -> ResponseResult<()> {
    // Parse language and code
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    if parts.len() < 2 {
        bot.send_message(chat_id, "Usage: /run <language> <code>")
            .await?;
        return Ok(());
    }

    let language: Language = match parts[0].parse() {
        Ok(lang) => lang,
        Err(_) => {
            bot.send_message(
                chat_id,
                format!(
                    "Unsupported language: {}\nSupported: python, javascript, bash, typescript",
                    parts[0]
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let code = parts[1];

    // Show typing
    bot.send_chat_action(chat_id, teloxide::types::ChatAction::Typing)
        .await?;

    // Execute code
    let request = ExecutionRequest::new(code, language);
    match state.executor.execute(request).await {
        Ok(result) => {
            let output = if result.success {
                format!(
                    "✅ *Execution successful*\n\n```\n{}\n```\n\n_Time: {:?}_",
                    escape_markdown(&result.stdout),
                    result.execution_time
                )
            } else if result.timed_out {
                "⏱️ *Execution timed out*".to_string()
            } else {
                format!(
                    "❌ *Execution failed*\n\n```\n{}\n```",
                    escape_markdown(&result.stderr)
                )
            };

            bot.send_message(chat_id, output)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
        Err(e) => {
            bot.send_message(chat_id, format!("❌ Execution error: {}", e))
                .await?;
        }
    }

    Ok(())
}

/// Handle document uploads
async fn handle_document(
    bot: Bot,
    msg: Message,
    _state: Arc<AppState>,
    _user_id: &str,
) -> ResponseResult<()> {
    let chat_id = msg.chat.id;
    let file_name = msg.document()
        .and_then(|d| d.file_name.as_deref())
        .unwrap_or("unknown");

    bot.send_message(
        chat_id,
        format!("📄 Received file: {}\nFile handling coming soon!", file_name),
    )
    .await?;

    Ok(())
}

/// Send a long message, splitting if necessary
async fn send_long_message(bot: &Bot, chat_id: ChatId, text: &str) -> ResponseResult<()> {
    const MAX_LENGTH: usize = 4096;

    if text.len() <= MAX_LENGTH {
        bot.send_message(chat_id, text).await?;
    } else {
        // Split into chunks - use String instead of &str to avoid borrowing issues
        let chars: Vec<char> = text.chars().collect();
        let chunks: Vec<String> = chars
            .chunks(MAX_LENGTH)
            .map(|c| c.iter().collect::<String>())
            .collect();

        for (i, chunk) in chunks.iter().enumerate() {
            bot.send_message(chat_id, format!("({}/{}) {}", i + 1, chunks.len(), chunk))
                .await?;
        }
    }

    Ok(())
}

/// Escape special characters for MarkdownV2
fn escape_markdown(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('_', "\\_")
        .replace('*', "\\*")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace('~', "\\~")
        .replace('`', "\\`")
        .replace('>', "\\>")
        .replace('#', "\\#")
        .replace('+', "\\+")
        .replace('-', "\\-")
        .replace('=', "\\=")
        .replace('|', "\\|")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('.', "\\.")
        .replace('!', "\\!")
}

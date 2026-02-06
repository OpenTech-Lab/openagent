//! OpenAgent Telegram Gateway
//!
//! The main entry point for the Telegram bot interface.
//! Implements OpenClaw-style session sandboxing and DM pairing.

use openagent::agent::{
    ConversationManager, GenerationOptions, Message as AgentMessage, OpenRouterClient,
    ToolRegistry, ToolCall, ReadFileTool, WriteFileTool, SystemCommandTool,
    DuckDuckGoSearchTool, BraveSearchTool, PerplexitySearchTool,
    prompts::DEFAULT_SYSTEM_PROMPT,
};
use openagent::config::Config;
use openagent::config::DmPolicy;
use openagent::database::init_pool;
use openagent::memory::{EmbeddingService, MemoryCache, MemoryRetriever};
use openagent::sandbox::{create_executor, CodeExecutor, ExecutionRequest, Language};
use openagent::{Error, Result};

use secrecy::ExposeSecret;
use std::collections::HashSet;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{ChatKind, ParseMode};
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
    #[command(description = "Approve a user (admin only, e.g., /approve 123456789)")]
    Approve(String),
    #[command(description = "List pending pairing requests (admin only)")]
    Pending,
}

/// Session type for sandboxing decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionType {
    /// Direct message (private chat) - trusted, runs on host
    DirectMessage,
    /// Group chat - sandboxed, restricted commands
    Group,
}

/// Pairing state for DM users
#[derive(Debug, Clone)]
struct PairingManager {
    /// Approved user IDs
    approved_users: HashSet<i64>,
    /// Pending pairing requests: user_id -> pairing_code
    pending_requests: std::collections::HashMap<i64, String>,
    /// Admin user IDs (can approve others)
    admin_users: HashSet<i64>,
}

impl PairingManager {
    fn new(admin_users: Vec<i64>) -> Self {
        let mut approved = HashSet::new();
        // Admins are automatically approved
        for admin in &admin_users {
            approved.insert(*admin);
        }

        PairingManager {
            approved_users: approved,
            pending_requests: std::collections::HashMap::new(),
            admin_users: admin_users.into_iter().collect(),
        }
    }

    /// Check if user is approved
    fn is_approved(&self, user_id: i64) -> bool {
        self.approved_users.contains(&user_id) || self.admin_users.contains(&user_id)
    }

    /// Check if user is admin
    fn is_admin(&self, user_id: i64) -> bool {
        self.admin_users.contains(&user_id)
    }

    /// Generate pairing code for a user
    fn generate_pairing_code(&mut self, user_id: i64) -> String {
        let code = format!("{:06}", rand::random::<u32>() % 1_000_000);
        self.pending_requests.insert(user_id, code.clone());
        code
    }

    /// Approve a user
    fn approve_user(&mut self, user_id: i64) -> bool {
        self.pending_requests.remove(&user_id);
        self.approved_users.insert(user_id)
    }

    /// Get pending requests (returns owned data)
    fn pending_users(&self) -> Vec<(i64, String)> {
        self.pending_requests
            .iter()
            .map(|(id, code)| (*id, code.clone()))
            .collect()
    }
}

/// Application state shared across handlers
struct AppState {
    config: Config,
    llm_client: OpenRouterClient,
    conversations: RwLock<ConversationManager>,
    memory_retriever: Option<MemoryRetriever>,
    executor: Box<dyn CodeExecutor>,
    /// Tools for DM sessions (full access)
    dm_tools: ToolRegistry,
    /// Tools for group sessions (sandboxed)
    group_tools: ToolRegistry,
    /// Pairing manager for DM approval
    pairing: RwLock<PairingManager>,
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

        // Try to initialize database + memory retriever (optional)
        let memory_retriever = match &config.storage.postgres {
            Some(db_config) => match init_pool(db_config).await {
                Ok(pool) => {
                    let store = openagent::database::MemoryStore::new(pool);
                    match EmbeddingService::new() {
                        Ok(embedding) => {
                            let cache = MemoryCache::new();
                            info!("Memory retriever initialized (embedding + cache + PG)");
                            Some(MemoryRetriever::new(store, embedding, cache))
                        }
                        Err(e) => {
                            warn!("Embedding service failed: {}. Running without memory retrieval.", e);
                            None
                        }
                    }
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

        // Initialize DM tools (full access for trusted users)
        let mut dm_tools = ToolRegistry::new();
        dm_tools.register(ReadFileTool::new(config.sandbox.allowed_dir.clone()));
        dm_tools.register(WriteFileTool::new(config.sandbox.allowed_dir.clone()));
        // DM: SystemCommandTool configured based on execution environment
        // OS mode: full access (no denylist, shell metacharacters allowed)
        // Sandbox/Container mode: default denylist (dangerous commands blocked)
        dm_tools.register(SystemCommandTool::with_config_and_env(
            config.sandbox.allowed_dir.clone(),
            config.sandbox.agent_user.clone(),
            &config.sandbox.execution_env.to_string(),
        ));
        dm_tools.register(DuckDuckGoSearchTool::new());
        if let Some(brave) = BraveSearchTool::from_env() {
            info!("Brave Search enabled for DM sessions");
            dm_tools.register(brave);
        }
        if let Some(perplexity) = PerplexitySearchTool::from_env() {
            info!("Perplexity Search enabled for DM sessions");
            dm_tools.register(perplexity);
        }

        // Initialize group tools (sandboxed - restricted commands)
        let mut group_tools = ToolRegistry::new();
        group_tools.register(ReadFileTool::new(config.sandbox.allowed_dir.clone()));
        // Groups: SystemCommandTool with strict allowlist (only safe read commands)
        let group_system_cmd = SystemCommandTool::with_working_dir(config.sandbox.allowed_dir.clone())
            .with_allowed_commands(vec![
                "ls".to_string(),
                "cat".to_string(),
                "head".to_string(),
                "tail".to_string(),
                "wc".to_string(),
                "grep".to_string(),
                "find".to_string(),
                "echo".to_string(),
                "pwd".to_string(),
                "whoami".to_string(),
                "date".to_string(),
                "uname".to_string(),
            ]);
        group_tools.register(group_system_cmd);
        group_tools.register(DuckDuckGoSearchTool::new());

        // Initialize pairing manager with admin users from config
        let admin_users = config.channels.telegram
            .as_ref()
            .map(|t| t.allow_from.clone())
            .unwrap_or_default();
        let pairing = PairingManager::new(admin_users);

        info!("DM tools: {} available", dm_tools.count());
        info!("Group tools: {} available (sandboxed)", group_tools.count());

        Ok(AppState {
            config,
            llm_client,
            conversations: RwLock::new(conversations),
            memory_retriever,
            executor,
            dm_tools,
            group_tools,
            pairing: RwLock::new(pairing),
        })
    }

    /// Get the appropriate tool registry based on session type
    fn tools_for_session(&self, session_type: SessionType) -> &ToolRegistry {
        match session_type {
            SessionType::DirectMessage => &self.dm_tools,
            SessionType::Group => &self.group_tools,
        }
    }
}

/// Determine session type from chat
fn get_session_type(chat: &teloxide::types::Chat) -> SessionType {
    match &chat.kind {
        ChatKind::Private(_) => SessionType::DirectMessage,
        _ => SessionType::Group, // Group, Supergroup, Channel
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
        Some(cfg) => {
            let token = cfg.bot_token.expose_secret();
            // Check for empty or placeholder tokens
            if token.is_empty() {
                warn!("TELEGRAM_BOT_TOKEN is empty, Telegram bot will not start");
                None
            } else if token.contains("your") || token.len() < 20 {
                warn!("TELEGRAM_BOT_TOKEN appears to be a placeholder, Telegram bot will not start");
                warn!("Get a real token from @BotFather on Telegram");
                None
            } else {
                Some(cfg)
            }
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
    let mut telegram_started = false;
    if let Some(telegram_config) = telegram_config {
        // Create bot
        let bot = Bot::new(telegram_config.bot_token.expose_secret());

        // Try to get bot info - if this fails, the token is invalid
        match bot.get_me().await {
            Ok(me) => {
                info!("Telegram bot started: @{}", me.username.as_deref().unwrap_or("unknown"));
                telegram_started = true;

                // Start dispatcher
                let handler = dptree::entry()
                    .branch(Update::filter_message().endpoint(message_handler));

                Dispatcher::builder(bot, handler)
                    .dependencies(dptree::deps![state])
                    .enable_ctrlc_handler()
                    .build()
                    .dispatch()
                    .await;
            }
            Err(e) => {
                error!("Failed to start Telegram bot: {}", e);
                warn!("Check your TELEGRAM_BOT_TOKEN - it may be invalid or revoked");
                warn!("Gateway will continue in standby mode without Telegram");
            }
        }
    }

    if !telegram_started {
        info!("No channels active. Gateway running in standby mode.");
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
    let user_id = msg.from.as_ref().map(|u| u.id.0 as i64).unwrap_or(0);
    let chat_id = msg.chat.id;
    let session_type = get_session_type(&msg.chat);

    // For DMs, check if user is approved (pairing system)
    if session_type == SessionType::DirectMessage {
        let dm_policy = state.config.channels.telegram
            .as_ref()
            .map(|t| t.dm_policy)
            .unwrap_or(DmPolicy::Open);

        let needs_pairing = match dm_policy {
            DmPolicy::Open => false,
            DmPolicy::Disabled => {
                bot.send_message(chat_id, "❌ DMs are disabled for this bot.")
                    .await?;
                return Ok(());
            }
            _ => !state.pairing.read().await.is_approved(user_id),
        };

        if needs_pairing {
            // Check if this is an admin command that doesn't require approval
            if let Some(text) = msg.text() {
                if !text.starts_with('/') {
                    // Not a command - require pairing
                    return handle_pairing_request(bot, msg, state, user_id).await;
                }
            } else {
                return handle_pairing_request(bot, msg, state, user_id).await;
            }
        }
    }

    // Handle commands
    if let Some(text) = msg.text() {
        let text = text.to_string();
        if text.starts_with('/') {
            return handle_command(bot, msg, state, &text, session_type).await;
        }

        // Regular message - chat with LLM
        return handle_chat(bot, msg, state, &text, &user_id.to_string(), session_type).await;
    }

    // Handle documents/files
    if msg.document().is_some() {
        return handle_document(bot, msg, state, &user_id.to_string()).await;
    }

    Ok(())
}

/// Handle pairing request for unapproved DM users
async fn handle_pairing_request(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
    user_id: i64,
) -> ResponseResult<()> {
    let chat_id = msg.chat.id;
    let username = msg.from.as_ref()
        .and_then(|u| u.username.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    // Generate pairing code
    let code = state.pairing.write().await.generate_pairing_code(user_id);

    info!("Pairing request from user {} (@{}), code: {}", user_id, username, code);

    bot.send_message(
        chat_id,
        format!(
            "🔐 *Pairing Required*\n\n\
            You need to be approved before using this bot\\.\n\n\
            Your pairing code: `{}`\n\
            User ID: `{}`\n\n\
            Please contact an administrator to approve your access\\.",
            code, user_id
        ),
    )
    .parse_mode(ParseMode::MarkdownV2)
    .await?;

    Ok(())
}

/// Handle bot commands
async fn handle_command(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
    text: &str,
    session_type: SessionType,
) -> ResponseResult<()> {
    let chat_id = msg.chat.id;
    let user_id = msg.from.as_ref().map(|u| u.id.0 as i64).unwrap_or(0);

    // Parse command
    let parts: Vec<&str> = text.splitn(2, ' ').collect();
    let cmd = parts[0].trim_start_matches('/').to_lowercase();
    // Remove @botname suffix if present
    let cmd = cmd.split('@').next().unwrap_or(&cmd);
    let args = parts.get(1).map(|s| s.to_string()).unwrap_or_default();

    match cmd {
        "start" => {
            let session_info = match session_type {
                SessionType::DirectMessage => "DM (full access)",
                SessionType::Group => "Group (sandboxed)",
            };
            bot.send_message(
                chat_id,
                format!(
                    "👋 Welcome to OpenAgent!\n\n\
                    I'm your AI assistant powered by OpenRouter. \
                    I can help you with coding, answer questions, and execute code.\n\n\
                    Session type: {}\n\n\
                    Use /help to see available commands.",
                    session_info
                ),
            )
            .await?;
        }
        "help" => {
            bot.send_message(chat_id, Command::descriptions().to_string())
                .await?;
        }
        "clear" => {
            let mut conversations = state.conversations.write().await;
            conversations.clear_conversation(&user_id.to_string());
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
                .get(&user_id.to_string())
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
                let conv = conversations.get_or_create(&user_id.to_string());
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
            let tools = state.tools_for_session(session_type);
            let session_info = match session_type {
                SessionType::DirectMessage => "DM (full access)",
                SessionType::Group => "Group (sandboxed)",
            };
            let status = format!(
                "🤖 *OpenAgent Status*\n\n\
                Version: {}\n\
                Model: {}\n\
                Session: {}\n\
                Execution: {}\n\
                Database: {}\n\
                Tools: {}",
                openagent::VERSION,
                default_model,
                session_info,
                state.config.sandbox.execution_env,
                if state.memory_retriever.is_some() { "Connected" } else { "Not connected" },
                tools.count()
            );
            bot.send_message(chat_id, status)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
        "approve" => {
            // Admin only command
            let is_admin = state.pairing.read().await.is_admin(user_id);
            if !is_admin {
                bot.send_message(chat_id, "❌ Only administrators can approve users.")
                    .await?;
                return Ok(());
            }

            if args.is_empty() {
                bot.send_message(chat_id, "Usage: /approve <user_id>")
                    .await?;
            } else {
                match args.trim().parse::<i64>() {
                    Ok(target_user_id) => {
                        let approved = state.pairing.write().await.approve_user(target_user_id);
                        if approved {
                            bot.send_message(chat_id, format!("✅ User {} has been approved.", target_user_id))
                                .await?;
                            info!("Admin {} approved user {}", user_id, target_user_id);
                        } else {
                            bot.send_message(chat_id, format!("ℹ️ User {} was already approved.", target_user_id))
                                .await?;
                        }
                    }
                    Err(_) => {
                        bot.send_message(chat_id, "❌ Invalid user ID. Please provide a numeric ID.")
                            .await?;
                    }
                }
            }
        }
        "pending" => {
            // Admin only command
            let pairing = state.pairing.read().await;
            if !pairing.is_admin(user_id) {
                bot.send_message(chat_id, "❌ Only administrators can view pending requests.")
                    .await?;
                return Ok(());
            }

            let pending = pairing.pending_users();
            drop(pairing); // Release the lock before sending messages

            if pending.is_empty() {
                bot.send_message(chat_id, "No pending pairing requests.")
                    .await?;
            } else {
                let mut response_msg = "📋 *Pending Pairing Requests:*\n\n".to_string();
                for (uid, code) in pending {
                    response_msg.push_str(&format!("• User `{}` \\- Code: `{}`\n", uid, code));
                }
                response_msg.push_str("\nUse `/approve <user_id>` to approve\\.");
                bot.send_message(chat_id, response_msg)
                    .parse_mode(ParseMode::MarkdownV2)
                    .await?;
            }
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
    session_type: SessionType,
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

    // Inject relevant memories into system prompt
    if let Some(ref retriever) = state.memory_retriever {
        match retriever.retrieve(user_id, text, 5).await {
            Ok(memory_context) if !memory_context.is_empty() => {
                if let Some(sys) = messages.iter_mut().find(|m| m.role == openagent::agent::Role::System) {
                    sys.content.push_str(&memory_context);
                    info!("Injected memory context ({} chars) for user={}", memory_context.len(), user_id);
                }
            }
            Err(e) => warn!("Memory retrieval failed: {}", e),
            _ => {}
        }
    }

    // Get tool definitions based on session type
    let tools = state.tools_for_session(session_type);
    let tool_definitions = tools.definitions();

    let session_label = match session_type {
        SessionType::DirectMessage => "DM",
        SessionType::Group => "Group",
    };
    info!("Starting agent loop ({}) with {} tools available", session_label, tool_definitions.len());

    // Maximum iterations to prevent infinite loops
    // Increased to support multi-step tasks (e.g., apt update, apt install, service start)
    const MAX_ITERATIONS: u32 = 20;
    let mut iteration = 0;
    let mut final_response = String::new();
    let mut tool_calls_made = 0u32;
    const MAX_TOOL_CALLS: u32 = 20;

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

                    // Execute each tool call
                    for tool_call in tool_calls.iter() {
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

                        info!("Executing tool: {} ({}) (call #{}/{})", tool_name, session_label, tool_calls_made, MAX_TOOL_CALLS);

                        // Show typing while executing tool
                        let _ = bot.send_chat_action(chat_id, teloxide::types::ChatAction::Typing).await;

                        // Execute the tool using session-appropriate registry
                        let call = ToolCall {
                            id: tool_call.id.clone(),
                            name: tool_name.clone(),
                            arguments: args,
                        };

                        let result = tools.execute(&call).await;

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

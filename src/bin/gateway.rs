//! OpenAgent Telegram Gateway
//!
//! The main entry point for the Telegram bot interface.

use openagent::agent::{
    Conversation, ConversationManager, GenerationOptions, Message as AgentMessage, OpenRouterClient,
    ToolRegistry, ReadFileTool, WriteFileTool, prompts::DEFAULT_SYSTEM_PROMPT,
};
use openagent::config::Config;
use openagent::database::{init_pool, MemoryStore, OpenSearchClient};
use openagent::sandbox::{create_executor, CodeExecutor, ExecutionRequest, Language};
use openagent::{Error, Result};

use secrecy::ExposeSecret;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{Message, ParseMode};
use teloxide::utils::command::BotCommands;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Bot commands
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
        // Initialize OpenRouter client
        let llm_client = OpenRouterClient::new(config.openrouter.clone())?;

        // Initialize conversation manager
        let conversations = ConversationManager::new(&config.openrouter.default_model)
            .with_system_prompt(DEFAULT_SYSTEM_PROMPT);

        // Try to initialize database (optional)
        let memory_store = match init_pool(&config.database).await {
            Ok(pool) => {
                // Try to initialize OpenSearch
                let opensearch = match OpenSearchClient::new(&config.opensearch).await {
                    Ok(os) => Some(os),
                    Err(e) => {
                        warn!("OpenSearch not available: {}. Using PostgreSQL only.", e);
                        None
                    }
                };
                Some(MemoryStore::new(pool, opensearch))
            }
            Err(e) => {
                warn!("Database not available: {}. Running without persistence.", e);
                None
            }
        };

        // Initialize code executor
        let executor = create_executor(&config.sandbox).await?;

        // Initialize tool registry
        let mut tools = ToolRegistry::new();
        tools.register(ReadFileTool::new(config.sandbox.allowed_dir.clone()));
        tools.register(WriteFileTool::new(config.sandbox.allowed_dir.clone()));

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

    // Validate required config
    if config.telegram.bot_token.expose_secret().is_empty() {
        return Err(Error::Config("TELEGRAM_BOT_TOKEN is required".to_string()));
    }

    // Initialize application state
    let state = Arc::new(AppState::new(config.clone()).await?);

    info!(
        "Initialized with model: {}",
        config.openrouter.default_model
    );
    info!(
        "Execution environment: {}",
        config.sandbox.execution_env
    );

    // Create bot
    let bot = Bot::new(config.telegram.bot_token.expose_secret());

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
    if !state.config.telegram.allowed_users.is_empty() {
        let user_id_num: i64 = user_id.parse().unwrap_or(0);
        if !state.config.telegram.allowed_users.contains(&user_id_num) {
            bot.send_message(chat_id, "You are not authorized to use this bot.")
                .await?;
            return Ok(());
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
            let model = conversations
                .get(&user_id)
                .map(|c| c.model.as_str())
                .unwrap_or(&state.config.openrouter.default_model);
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
            let status = format!(
                "🤖 *OpenAgent Status*\n\n\
                Version: {}\n\
                Model: {}\n\
                Execution: {}\n\
                Database: {}\n\
                Tools: {}",
                openagent::VERSION,
                state.config.openrouter.default_model,
                state.config.sandbox.execution_env,
                if state.memory_store.is_some() { "Connected" } else { "Not connected" },
                state.tools.count()
            );
            bot.send_message(chat_id, status)
                .parse_mode(ParseMode::Markdown)
                .await?;
        }
        _ => {
            bot.send_message(chat_id, "Unknown command. Use /help to see available commands.")
                .await?;
        }
    }

    Ok(())
}

/// Handle regular chat messages
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

    // Get or create conversation
    let messages = {
        let mut conversations = state.conversations.write().await;
        let conv = conversations.get_or_create(user_id);
        conv.add_user_message(text);
        conv.get_api_messages()
    };

    // Generate response
    match state
        .llm_client
        .chat(messages, GenerationOptions::balanced())
        .await
    {
        Ok(response) => {
            if let Some(choice) = response.choices.first() {
                let reply = &choice.message.content;

                // Store assistant response
                {
                    let mut conversations = state.conversations.write().await;
                    if let Some(conv) = conversations.get_mut(user_id) {
                        conv.add_assistant_message(reply);
                        if let Some(usage) = &response.usage {
                            conv.total_tokens += usage.total_tokens;
                        }
                    }
                }

                // Send response (split if too long)
                send_long_message(&bot, chat_id, reply).await?;
            }
        }
        Err(e) => {
            error!("LLM error: {}", e);
            bot.send_message(chat_id, format!("❌ Error: {}", e))
                .await?;
        }
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

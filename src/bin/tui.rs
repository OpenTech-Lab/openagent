//! OpenAgent TUI - Interactive Terminal Interface
//!
//! A local development interface for testing the agent without Telegram.
//! Provides full DM-level access to tools in a REPL environment.
//! Optionally connects to PostgreSQL for persistent memory.

use openagent::agent::{
    Conversation, LoopConfig, Role,
    OpenRouterClient, ToolRegistry, ReadFileTool, WriteFileTool,
    SystemCommandTool, DuckDuckGoSearchTool, BraveSearchTool, PerplexitySearchTool,
    MemorySaveTool, MemorySearchTool, MemoryListTool, MemoryDeleteTool,
    prompts::Soul,
    agentic_loop::{self, AgentLoopInput, LoopCallback, ToolObservation},
};
use openagent::config::Config;
use openagent::database::{init_pool, Memory, MemoryType};
use openagent::memory::{ConversationSummarizer, EmbeddingService, MemoryCache, MemoryRetriever};
use openagent::{Error, Result};

use clap::Parser;
use console::{style, Term};
use dialoguer::{theme::ColorfulTheme, Input};
use std::io::{self, Write};
use tracing::{info, warn};

/// OpenAgent TUI - Interactive Terminal Agent
#[derive(Parser, Debug)]
#[command(name = "openagent-tui")]
#[command(about = "Interactive terminal interface for OpenAgent")]
#[command(version)]
struct Args {
    /// Model to use (overrides .env default)
    #[arg(short, long)]
    model: Option<String>,

    /// Disable tools (chat-only mode)
    #[arg(long)]
    no_tools: bool,

    /// Show verbose tool arguments
    #[arg(short, long)]
    verbose: bool,

    /// Enable persistent memory (requires DATABASE_URL)
    #[arg(long)]
    memory: bool,
}

/// TUI application state
struct TuiState {
    config: Config,
    llm_client: OpenRouterClient,
    conversation: Conversation,
    tools: ToolRegistry,
    current_model: String,
    verbose: bool,
    tools_enabled: bool,
    memory_retriever: Option<MemoryRetriever>,
    user_id: String,
}

impl TuiState {
    async fn new(args: &Args) -> Result<Self> {
        let config = Config::from_env()?;

        // Initialize LLM client
        let openrouter_config = config.provider.openrouter.clone()
            .ok_or_else(|| Error::Config("OpenRouter not configured".into()))?;
        let llm_client = OpenRouterClient::new(openrouter_config.clone())?;

        // Determine model
        let current_model = args.model.clone()
            .unwrap_or_else(|| openrouter_config.default_model.clone());

        // Load soul for system prompt
        let soul = Soul::load_or_default();
        let system_prompt = soul.as_system_prompt();

        // User ID for memory
        let user_id = "tui-user".to_string();

        // Create conversation
        let conversation = Conversation::new(&user_id, &current_model)
            .with_system_prompt(&system_prompt);

        // Initialize memory retriever if requested
        let memory_retriever = if args.memory {
            match &config.storage.postgres {
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
                    warn!("DATABASE_URL not configured. Running without persistence.");
                    None
                }
            }
        } else {
            None
        };

        // Initialize tools (DM-level access)
        let mut tools = ToolRegistry::new();
        if !args.no_tools {
            tools.register(ReadFileTool::new(config.sandbox.allowed_dir.clone()));
            tools.register(WriteFileTool::new(config.sandbox.allowed_dir.clone()));
            tools.register(SystemCommandTool::with_config_and_env(
                config.sandbox.allowed_dir.clone(),
                config.sandbox.agent_user.clone(),
                &config.sandbox.execution_env.to_string(),
            ));
            tools.register(DuckDuckGoSearchTool::new());
            
            if let Some(brave) = BraveSearchTool::from_env() {
                info!("Brave Search enabled");
                tools.register(brave);
            }
            if let Some(perplexity) = PerplexitySearchTool::from_env() {
                info!("Perplexity Search enabled");
                tools.register(perplexity);
            }

            // Register memory tools if memory retriever is available
            if let Some(ref retriever) = memory_retriever {
                tools.register(MemorySaveTool::new(retriever.clone()));
                tools.register(MemorySearchTool::new(retriever.clone()));
                tools.register(MemoryListTool::new(retriever.clone()));
                tools.register(MemoryDeleteTool::new(retriever.clone()));
                info!("Memory tools registered");
            }
        }

        Ok(TuiState {
            config,
            llm_client,
            conversation,
            tools,
            current_model,
            verbose: args.verbose,
            tools_enabled: !args.no_tools,
            memory_retriever,
            user_id,
        })
    }

    /// Save a message to memory (with embedding)
    async fn save_to_memory(&self, content: &str, role: &str) {
        if let Some(ref retriever) = self.memory_retriever {
            let memory = Memory::new(&self.user_id, content)
                .with_tags(vec![role.to_string(), "tui".to_string()]);
            if let Err(e) = retriever.save_memory(&memory).await {
                warn!("Failed to save memory: {}", e);
            }
        }
    }

    /// Search memories (full-text via retriever's underlying store)
    async fn search_memories(&self, query: &str, limit: usize) -> Vec<Memory> {
        if let Some(ref retriever) = self.memory_retriever {
            match retriever.store().search_fulltext(&self.user_id, query, limit).await {
                Ok(memories) => memories,
                Err(e) => {
                    warn!("Failed to search memories: {}", e);
                    vec![]
                }
            }
        } else {
            vec![]
        }
    }
}

fn theme() -> ColorfulTheme {
    ColorfulTheme::default()
}

fn print_banner() {
    println!();
    println!("{}", style("╔══════════════════════════════════════════════════════════════╗").cyan());
    println!("{}", style("║             🤖 OpenAgent TUI - Local Agent Interface         ║").cyan());
    println!("{}", style("╚══════════════════════════════════════════════════════════════╝").cyan());
    println!();
}

fn print_help(has_memory: bool) {
    println!();
    println!("   {}", style("Available Commands:").cyan().bold());
    println!("   {}    - Exit TUI", style("/quit").yellow());
    println!("   {}   - Clear conversation history", style("/clear").yellow());
    println!("   {}   - List available tools", style("/tools").yellow());
    println!("   {}   - Show current model", style("/model").yellow());
    println!("   {} - Toggle verbose mode", style("/verbose").yellow());
    println!("   {} - Show conversation history", style("/history").yellow());
    if has_memory {
        println!("   {}  - Search memories (e.g., /search rust)", style("/search").yellow());
        println!("   {}  - Show memory status", style("/memory").yellow());
    }
    println!("   {}    - Show this help", style("/help").yellow());
    println!();
}

fn print_tools(state: &TuiState) {
    println!();
    if !state.tools_enabled {
        println!("   {} Tools are disabled (--no-tools mode)", style("ℹ").blue());
    } else {
        println!("   {} {} tools available:", style("🛠").bold(), state.tools.count());
        println!();
        for def in state.tools.definitions() {
            println!("   {} {}", style("•").cyan(), style(&def.function.name).green().bold());
            println!("     {}", style(&def.function.description).dim());
        }
    }
    println!();
}

fn print_history(state: &TuiState) {
    println!();
    println!("   {}", style("Conversation History:").cyan().bold());
    println!();
    
    let messages = state.conversation.get_api_messages();
    let mut count = 0;
    
    for msg in &messages {
        if msg.role == Role::System {
            continue; // Skip system prompt
        }
        count += 1;
        let role_str = msg.role.to_string();
        let role_style = match msg.role {
            Role::User => style(&role_str).green().bold(),
            Role::Assistant => style(&role_str).cyan().bold(),
            Role::Tool => style(&role_str).yellow().bold(),
            _ => style(&role_str).dim(),
        };
        
        let content_preview = if msg.content.len() > 100 {
            format!("{}...", &msg.content[..100])
        } else {
            msg.content.clone()
        };
        
        println!("   {} [{}]: {}", style(format!("{:02}", count)).dim(), role_style, content_preview);
    }
    
    if count == 0 {
        println!("   {}", style("(empty)").dim());
    }
    println!();
}

/// Format tool result for display
fn format_tool_result(result: &str, max_len: usize) -> String {
    if result.len() > max_len {
        format!("{}... ({} chars)", &result[..max_len], result.len())
    } else {
        result.to_string()
    }
}

/// Callback for the TUI agentic loop: prints thinking indicator and tool results.
struct TuiCallback;

#[async_trait::async_trait]
impl LoopCallback for TuiCallback {
    async fn on_iteration_start(&self, _iteration: u32) {
        print!("   {} ", style("●●●").dim());
        let _ = io::stdout().flush();
    }

    async fn on_tool_executed(&self, tool_name: &str, observation: &ToolObservation) {
        // Clear thinking indicator on first tool result
        let term = Term::stdout();
        let _ = term.clear_line();
        print!("\r");

        // Build a pseudo ToolCall for display
        let emoji = match tool_name {
            "read_file" => "📖",
            "write_file" => "✏️",
            "system_command" => "⚡",
            "duckduckgo_search" | "brave_search" | "perplexity_search" => "🔍",
            _ => "🔧",
        };
        println!("   {} {}", emoji, style(tool_name).yellow().bold());

        if observation.success {
            println!("   {} {}", style("✓").green(), format_tool_result(&observation.content, 200));
        } else {
            println!("   {} {}", style("✗").red(), format_tool_result(&observation.content, 200));
        }

        if observation.loop_guard_triggered {
            println!("   {} {}", style("⚠").yellow(), style("Repetition detected, forcing reconsideration").dim());
        }
    }

    async fn on_iteration_end(&self, _step: &agentic_loop::LoopStep) {
        // Clear thinking indicator if no tools were called this iteration
        let term = Term::stdout();
        let _ = term.clear_line();
        print!("\r");
    }
}

/// Run the agent loop with tool support
async fn agent_loop(state: &mut TuiState, user_input: &str) -> Result<String> {
    // Add user message to conversation
    state.conversation.add_user_message(user_input);

    let mut messages = state.conversation.get_api_messages();

    // Inject relevant memories into system prompt
    if let Some(ref retriever) = state.memory_retriever {
        match retriever.retrieve(&state.user_id, user_input, 5).await {
            Ok(memory_context) if !memory_context.is_empty() => {
                if let Some(sys) = messages.iter_mut().find(|m| m.role == Role::System) {
                    sys.content.push_str(&memory_context);
                    info!("Injected memory context ({} chars)", memory_context.len());
                }
            }
            Err(e) => warn!("Memory retrieval failed: {}", e),
            _ => {}
        }
    }

    let tool_definitions = if state.tools_enabled {
        state.tools.definitions()
    } else {
        vec![]
    };

    let tui_callback = TuiCallback;

    let loop_input = AgentLoopInput {
        messages,
        llm_client: &state.llm_client,
        tools: &state.tools,
        tool_definitions,
        config: LoopConfig::tui(),
        user_id: Some(state.user_id.clone()),
        chat_id: None,
        callback: tui_callback,
    };

    let output = agentic_loop::run_agentic_loop(loop_input).await?;

    // Store assistant response in conversation
    if !output.response.is_empty() {
        state.conversation.add_assistant_message(&output.response);
    }

    Ok(output.response)
}

/// Main REPL loop
async fn run_repl(mut state: TuiState) -> Result<()> {
    print_banner();
    
    println!("   {} Model: {}", style("✓").green(), style(&state.current_model).cyan());
    println!("   {} Tools: {} available", style("✓").green(), state.tools.count());
    println!("   {} Working directory: {}", style("✓").green(), 
        style(state.config.sandbox.allowed_dir.display()).dim());
    
    // Show memory status
    let has_memory = state.memory_retriever.is_some();
    if has_memory {
        println!("   {} Memory: {}", style("✓").green(), style("PostgreSQL (persistent)").cyan());
    } else {
        println!("   {} Memory: {}", style("ℹ").blue(), style("session only (use --memory for persistence)").dim());
    }
    
    println!();
    println!("   Type {} for available commands.", style("/help").yellow());
    println!();
    
    loop {
        // Get user input
        let user_input: String = match Input::with_theme(&theme())
            .with_prompt(style("You").green().bold().to_string())
            .allow_empty(true)
            .interact_text()
        {
            Ok(input) => input,
            Err(e) => {
                if e.to_string().contains("interrupted") {
                    println!("\n{} Goodbye!\n", style("👋").bold());
                    break;
                }
                eprintln!("Input error: {}", e);
                continue;
            }
        };
        
        let input = user_input.trim();
        
        if input.is_empty() {
            continue;
        }
        
        // Handle commands
        if input.starts_with('/') {
            match input.to_lowercase().as_str() {
                "/quit" | "/exit" | "/q" => {
                    println!("\n{} Goodbye!\n", style("👋").bold());
                    break;
                }
                "/clear" | "/c" => {
                    // Auto-summarize before clearing if enough messages
                    if state.conversation.message_count() >= 4 {
                        if let Some(ref retriever) = state.memory_retriever {
                            let messages = state.conversation.messages.clone();
                            let summarizer = ConversationSummarizer::new(state.llm_client.clone());
                            let retriever = retriever.clone();
                            let uid = state.user_id.clone();
                            tokio::spawn(async move {
                                match summarizer.summarize(&messages).await {
                                    Ok(episodic) => {
                                        if episodic.summary.is_empty() {
                                            return;
                                        }
                                        let memory = Memory::new(&uid, &episodic.summary)
                                            .with_importance(0.6)
                                            .with_memory_type(MemoryType::Episodic)
                                            .with_source("auto:episodic")
                                            .with_tags(episodic.topics.clone());
                                        if let Err(e) = retriever.save_memory(&memory).await {
                                            warn!("Failed to save episodic memory: {}", e);
                                        } else {
                                            info!("Auto-episodic memory saved for user={}", uid);
                                        }
                                        for fact in &episodic.key_facts {
                                            let fact_memory = Memory::new(&uid, fact)
                                                .with_importance(0.7)
                                                .with_memory_type(MemoryType::Semantic)
                                                .with_source("auto:extracted")
                                                .with_tags(vec!["auto-extracted".into()]);
                                            let _ = retriever.save_memory(&fact_memory).await;
                                        }
                                        for pref in &episodic.user_preferences {
                                            let pref_memory = Memory::new(&uid, pref)
                                                .with_importance(0.8)
                                                .with_memory_type(MemoryType::Semantic)
                                                .with_source("auto:extracted")
                                                .with_tags(vec!["preference".into(), "auto-extracted".into()]);
                                            let _ = retriever.save_memory(&pref_memory).await;
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Auto-summarization failed: {}", e);
                                    }
                                }
                            });
                        }
                    }

                    let soul = Soul::load_or_default();
                    state.conversation = Conversation::new("tui-user", &state.current_model)
                        .with_system_prompt(soul.as_system_prompt());
                    let term = Term::stdout();
                    let _ = term.clear_screen();
                    print_banner();
                    println!("   {} Conversation cleared.\n", style("✓").green());
                    continue;
                }
                "/tools" | "/t" => {
                    print_tools(&state);
                    continue;
                }
                "/model" | "/m" => {
                    println!();
                    println!("   {} Current model: {}", style("ℹ").blue(), style(&state.current_model).cyan());
                    println!();
                    continue;
                }
                "/verbose" | "/v" => {
                    state.verbose = !state.verbose;
                    println!();
                    println!("   {} Verbose mode: {}", 
                        style("✓").green(), 
                        if state.verbose { style("ON").green() } else { style("OFF").yellow() });
                    println!();
                    continue;
                }
                "/history" | "/h" => {
                    print_history(&state);
                    continue;
                }
                "/help" | "/?" => {
                    print_help(has_memory);
                    continue;
                }
                cmd if cmd.starts_with("/search ") => {
                    if !has_memory {
                        println!("   {} Memory not enabled. Use {} flag.\n", 
                            style("⚠").yellow(), style("--memory").cyan());
                        continue;
                    }
                    let query = cmd.strip_prefix("/search ").unwrap_or("").trim();
                    if query.is_empty() {
                        println!("   {} Usage: /search <query>\n", style("ℹ").blue());
                        continue;
                    }
                    let memories = state.search_memories(query, 5).await;
                    println!();
                    if memories.is_empty() {
                        println!("   {} No memories found for: {}\n", style("ℹ").blue(), query);
                    } else {
                        println!("   {} Found {} memories:", style("🔍").bold(), memories.len());
                        for mem in memories {
                            let preview = if mem.content.len() > 100 {
                                format!("{}...", &mem.content[..100])
                            } else {
                                mem.content.clone()
                            };
                            println!("   {} [{}] {}", 
                                style("•").cyan(), 
                                style(mem.created_at.format("%Y-%m-%d %H:%M")).dim(),
                                preview);
                        }
                        println!();
                    }
                    continue;
                }
                "/memory" => {
                    println!();
                    if has_memory {
                        println!("   {} Memory: {} (PostgreSQL)", style("✓").green(), style("enabled").green());
                        println!("   {} User ID: {}", style("ℹ").blue(), style(&state.user_id).dim());
                    } else {
                        println!("   {} Memory: {} (session only)", style("ℹ").blue(), style("disabled").yellow());
                        println!("   {} Run with {} to enable persistence", style("💡").bold(), style("--memory").cyan());
                    }
                    println!();
                    continue;
                }
                _ => {
                    println!("   {} Unknown command. Type {} for help.\n", 
                        style("⚠").yellow(), style("/help").cyan());
                    continue;
                }
            }
        }
        
        // Run agent loop
        match agent_loop(&mut state, input).await {
            Ok(response) => {
                // Save to memory if enabled
                state.save_to_memory(input, "user").await;
                state.save_to_memory(&response, "assistant").await;
                
                println!();
                println!("   {}: {}", style("Agent").cyan().bold(), response);
                println!();
            }
            Err(e) => {
                println!();
                println!("   {} Error: {}", style("❌").red(), e);
                println!();
            }
        }
    }
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment
    dotenvy::dotenv().ok();
    
    // Initialize logging (quieter for TUI)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("openagent=info".parse().unwrap())
                .add_directive("warn".parse().unwrap())
        )
        .with_target(false)
        .without_time()
        .init();
    
    // Parse CLI args
    let args = Args::parse();
    
    // Initialize state
    let state = TuiState::new(&args).await?;
    
    // Run REPL
    run_repl(state).await
}

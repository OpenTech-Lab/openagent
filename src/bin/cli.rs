//! OpenAgent CLI
//!
//! Command-line interface for configuration, onboarding, and management.

use clap::{Parser, Subcommand};
use console::{style, Term};
use dialoguer::{theme::ColorfulTheme, Confirm, FuzzySelect, Input, MultiSelect, Password, Select};
use openagent::config::{Config, ExecutionEnv};
use openagent::database::{init_pool, init_pool_for_migrations, migrations, OpenSearchClient};
use openagent::{Error, Result, VERSION};
use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::net::TcpListener;
use std::path::Path;

// Remove unused import warnings
#[allow(unused_imports)]
use tracing::{error, info};

#[derive(Parser)]
#[command(
    name = "openagent",
    author = "OpenAgent Contributors",
    version = VERSION,
    about = "OpenAgent - High-performance AI agent framework",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize .env file with auto-configured port (run this first)
    Init {
        /// Force overwrite existing .env file
        #[arg(long, short)]
        force: bool,
    },

    /// Initialize OpenAgent and verify configuration
    Onboard {
        /// Install and enable the systemd daemon
        #[arg(long)]
        install_daemon: bool,
    },

    /// Check the status of all services
    Status,

    /// Run database migrations
    Migrate,

    /// Test the OpenRouter connection
    TestLlm {
        /// Model to test
        #[arg(short, long)]
        model: Option<String>,
    },

    /// Execute code in the sandbox
    Run {
        /// Programming language
        language: String,
        /// Code to execute
        code: String,
    },

    /// List available models
    Models,

    /// Generate a sample configuration
    InitConfig,

    /// Interactive chat mode
    Chat {
        /// Model to use
        #[arg(short, long)]
        model: Option<String>,
    },

    /// View or edit the agent's soul (personality configuration)
    Soul {
        #[command(subcommand)]
        action: Option<SoulAction>,
    },
}

#[derive(Subcommand)]
enum SoulAction {
    /// View the current soul
    View,
    /// Edit the soul in your default editor
    Edit,
    /// Reset to default soul
    Reset,
    /// Add a learned preference
    Learn {
        /// The preference or fact to remember
        text: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("openagent=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init { force }) => init_env(force),
        Some(Commands::Onboard { install_daemon }) => onboard(install_daemon).await,
        Some(Commands::Status) => check_status().await,
        Some(Commands::Migrate) => run_migrations().await,
        Some(Commands::TestLlm { model }) => test_llm(model).await,
        Some(Commands::Run { language, code }) => run_code(&language, &code).await,
        Some(Commands::Models) => list_models().await,
        Some(Commands::InitConfig) => init_config(),
        Some(Commands::Chat { model }) => interactive_chat(model).await,
        Some(Commands::Soul { action }) => manage_soul(action),
        None => interactive_main_menu().await,
    }
}

// ============================================================================
// Port Discovery and .env Management
// ============================================================================

const PORT_RANGE_START: u16 = 20000;
const PORT_RANGE_END: u16 = 29999;

/// Find a free port in the range 20000-29999 (random selection)
fn find_free_port() -> Option<u16> {
    let mut ports: Vec<u16> = (PORT_RANGE_START..=PORT_RANGE_END).collect();
    let mut rng = rand::rng();
    ports.shuffle(&mut rng);
    
    for port in ports {
        if is_port_available(port) {
            return Some(port);
        }
    }
    None
}

/// Check if a port is available by attempting to bind to it
fn is_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Read existing .env file into a HashMap
fn read_env_file(path: &Path) -> Result<HashMap<String, String>> {
    let mut env_vars = HashMap::new();

    if path.exists() {
        // Check if it's a directory (common misconfiguration)
        if path.is_dir() {
            return Err(Error::Config(format!(
                "{} is a directory, not a file. Please remove it with: rm -rf {}",
                path.display(),
                path.display()
            )));
        }
        let file = std::fs::File::open(path)?;
        let reader = io::BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse KEY=VALUE
            if let Some(pos) = line.find('=') {
                let key = line[..pos].trim().to_string();
                let value = line[pos + 1..].trim().to_string();
                env_vars.insert(key, value);
            }
        }
    }

    Ok(env_vars)
}

/// Write .env file with comments preserved where possible
fn write_env_file(path: &Path, vars: &HashMap<String, String>) -> Result<()> {
    use std::io::Write;

    // Check if it's a directory (common misconfiguration)
    if path.exists() && path.is_dir() {
        return Err(Error::Config(format!(
            "{} is a directory, not a file. Please remove it with: rm -rf {}",
            path.display(),
            path.display()
        )));
    }

    let template = include_str!("../../.env.example");
    let mut output = String::new();
    let mut written_keys = std::collections::HashSet::new();

    // Process template line by line, replacing values where we have them
    for line in template.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            // Keep comments and empty lines as-is
            output.push_str(line);
            output.push('\n');
            continue;
        }

        // Check if this is a KEY=VALUE line
        if let Some(pos) = trimmed.find('=') {
            let key = trimmed[..pos].trim();

            if let Some(value) = vars.get(key) {
                // Use our value
                output.push_str(&format!("{}={}\n", key, value));
                written_keys.insert(key.to_string());
            } else {
                // Keep original line
                output.push_str(line);
                output.push('\n');
            }
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }

    // Add any new keys that weren't in the template
    for (key, value) in vars {
        if !written_keys.contains(key) {
            output.push_str(&format!("{}={}\n", key, value));
        }
    }

    // Write with explicit sync to ensure data is flushed to disk
    // This is important for Docker bind mounts where writes may not be immediately visible
    let mut file = std::fs::File::create(path)?;
    file.write_all(output.as_bytes())?;
    file.sync_all()?;

    Ok(())
}

/// Create a new .env file from template with auto-configured values
fn create_env_file(port: u16) -> Result<()> {
    let env_path = Path::new(".env");

    // Start with empty vars (will use template defaults)
    let mut vars = HashMap::new();

    // Set auto-configured values
    vars.insert("WEBHOOK_PORT".to_string(), port.to_string());
    vars.insert("WEBHOOK_HOST".to_string(), "0.0.0.0".to_string());
    vars.insert("USE_LONG_POLLING".to_string(), "true".to_string());

    write_env_file(env_path, &vars)?;

    Ok(())
}

/// Update existing .env file with new port
#[allow(dead_code)]
fn update_env_port(port: u16) -> Result<()> {
    let env_path = Path::new(".env");
    let mut vars = read_env_file(env_path)?;

    vars.insert("WEBHOOK_PORT".to_string(), port.to_string());

    write_env_file(env_path, &vars)?;

    Ok(())
}

/// Get the dialoguer theme
fn theme() -> ColorfulTheme {
    ColorfulTheme::default()
}

/// Prompt user for input using dialoguer
fn prompt(message: &str) -> Result<String> {
    let input: String = Input::with_theme(&theme())
        .with_prompt(message.trim())
        .allow_empty(true)
        .interact_text()
        .map_err(|e| Error::Config(format!("Input error: {}", e)))?;
    Ok(input)
}

/// Prompt user for yes/no using dialoguer (arrow keys + enter)
fn prompt_yes_no(message: &str, default: bool) -> Result<bool> {
    let result = Confirm::with_theme(&theme())
        .with_prompt(message.trim())
        .default(default)
        .interact()
        .map_err(|e| Error::Config(format!("Confirm error: {}", e)))?;
    Ok(result)
}

/// Prompt for sensitive input (hidden with asterisks)
#[allow(dead_code)]
fn prompt_secret(message: &str) -> Result<String> {
    let password = Password::with_theme(&theme())
        .with_prompt(message.trim())
        .allow_empty_password(true)
        .interact()
        .map_err(|e| Error::Config(format!("Password error: {}", e)))?;
    Ok(password)
}

/// Prompt with a default value shown
fn prompt_with_default(message: &str, default: &str) -> Result<String> {
    let input: String = Input::with_theme(&theme())
        .with_prompt(message.trim())
        .default(default.to_string())
        .interact_text()
        .map_err(|e| Error::Config(format!("Input error: {}", e)))?;
    Ok(input)
}

/// Display an interactive menu with arrow key navigation
/// Use ↑/↓ to navigate, Enter to select
fn prompt_menu(title: &str, options: &[&str], default: usize) -> Result<usize> {
    println!("\n{}", style(title).cyan().bold());
    println!("{}", style("  Use ↑/↓ arrows to navigate, Enter to select").dim());

    let selection = Select::with_theme(&theme())
        .items(options)
        .default(default)
        .interact()
        .map_err(|e| Error::Config(format!("Selection error: {}", e)))?;

    Ok(selection)
}

/// Display a multi-select menu with arrow keys and space to toggle
/// Use ↑/↓ to navigate, Space to toggle, Enter to confirm
#[allow(dead_code)]
fn prompt_multi_select(title: &str, options: &[&str], defaults: &[bool]) -> Result<Vec<usize>> {
    println!("\n{}", style(title).cyan().bold());
    println!("{}", style("  Use ↑/↓ arrows, Space to toggle, Enter to confirm").dim());

    let selections = MultiSelect::with_theme(&theme())
        .items(options)
        .defaults(defaults)
        .interact()
        .map_err(|e| Error::Config(format!("Multi-select error: {}", e)))?;

    Ok(selections)
}

/// Print a section header
fn print_section(title: &str) {
    println!("\n{}", "─".repeat(50));
    println!("  {}", title);
    println!("{}", "─".repeat(50));
}

/// Print step indicator
fn print_step(step: usize, total: usize, title: &str) {
    println!("\n📍 Step {}/{}: {}", step, total, title);
}

// ============================================================================
// Interactive Main Menu
// ============================================================================

/// Interactive main menu when run without subcommands
async fn interactive_main_menu() -> Result<()> {
    loop {
        let term = Term::stdout();
        let _ = term.clear_screen();

        println!();
        println!("{}", style("╔══════════════════════════════════════════════════════════════╗").cyan());
        println!("{}", style("║                                                              ║").cyan());
        println!("{}", style("║           🚀  O P E N A G E N T                              ║").cyan());
        println!("{}", style("║                                                              ║").cyan());
        println!("{}", style("║     High-performance AI Agent Framework in Rust              ║").cyan());
        println!("{}", style("║                                                              ║").cyan());
        println!("{}", style("╚══════════════════════════════════════════════════════════════╝").cyan());
        println!();

        // Check configuration status
        let config_status = check_config_status();
        println!("   {}", config_status);
        println!();

        let menu_options = &[
            "💬  Start Interactive Chat",
            "🧠  View/Edit Agent Soul",
            "🔧  Run Setup Wizard (onboard)",
            "📊  Check Service Status",
            "🤖  Browse & Select Models",
            "🧪  Test LLM Connection",
            "📁  Initialize Environment (.env)",
            "🗃️   Run Database Migrations",
            "⚙️   Generate Config Sample",
            "❌  Exit",
        ];

        let selection = Select::with_theme(&theme())
            .with_prompt(format!("{}", style("What would you like to do?").bold()))
            .items(menu_options)
            .default(0)
            .interact()
            .map_err(|e| Error::Config(format!("Selection error: {}", e)))?;

        println!();

        match selection {
            0 => {
                interactive_chat(None).await?;
                println!("\nPress Enter to continue...");
                let _ = io::stdin().read_line(&mut String::new());
            }
            1 => {
                manage_soul(None)?;
                println!("\nPress Enter to continue...");
                let _ = io::stdin().read_line(&mut String::new());
            }
            2 => {
                onboard(false).await?;
                println!("\nPress Enter to continue...");
                let _ = io::stdin().read_line(&mut String::new());
            }
            3 => {
                check_status_interactive().await?;
                println!("\nPress Enter to continue...");
                let _ = io::stdin().read_line(&mut String::new());
            }
            4 => {
                list_models().await?;
                println!("\nPress Enter to continue...");
                let _ = io::stdin().read_line(&mut String::new());
            }
            5 => {
                test_llm(None).await?;
                println!("\nPress Enter to continue...");
                let _ = io::stdin().read_line(&mut String::new());
            }
            6 => {
                init_env_interactive()?;
                println!("\nPress Enter to continue...");
                let _ = io::stdin().read_line(&mut String::new());
            }
            7 => {
                run_migrations().await?;
                println!("\nPress Enter to continue...");
                let _ = io::stdin().read_line(&mut String::new());
            }
            8 => {
                init_config()?;
                println!("\nPress Enter to continue...");
                let _ = io::stdin().read_line(&mut String::new());
            }
            9 => {
                println!("{} Goodbye!\n", style("👋").bold());
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

/// Check configuration status for display
fn check_config_status() -> String {
    let env_path = Path::new(".env");
    
    if !env_path.exists() {
        return format!("{} No .env file found - run Setup Wizard first", style("⚠️ ").yellow());
    }

    match read_env_file(env_path) {
        Ok(vars) => {
            let has_openrouter = vars.get("OPENROUTER_API_KEY")
                .map(|k| !k.is_empty() && k != "your_openrouter_api_key_here")
                .unwrap_or(false);
            let has_telegram = vars.get("TELEGRAM_BOT_TOKEN")
                .map(|k| !k.is_empty() && k != "your_telegram_bot_token_here")
                .unwrap_or(false);
            let model = vars.get("DEFAULT_MODEL")
                .cloned()
                .unwrap_or_else(|| "not set".to_string());

            let status_icons = format!("OpenRouter: {} │ Telegram: {}",
                if has_openrouter { style("✓").green() } else { style("✗").red() },
                if has_telegram { style("✓").green() } else { style("✗").red() }
            );

            format!("{} │ Model: {}", status_icons, style(model).cyan())
        }
        Err(_) => format!("{} Error reading .env", style("❌").red()),
    }
}

/// Interactive version of init_env with prompts
fn init_env_interactive() -> Result<()> {
    let env_path = Path::new(".env");

    if env_path.exists() {
        if !prompt_yes_no("   .env already exists. Overwrite?", false)? {
            println!("   {} Cancelled.", style("ℹ").blue());
            return Ok(());
        }
    }

    init_env(true)
}

/// Interactive status check with options to fix issues
async fn check_status_interactive() -> Result<()> {
    println!();
    println!("{}", style("╔══════════════════════════════════════════════════╗").cyan());
    println!("{}", style("║           📊 OpenAgent Service Status            ║").cyan());
    println!("{}", style("╚══════════════════════════════════════════════════╝").cyan());
    println!();

    let config = match Config::from_env() {
        Ok(c) => {
            println!("   {} Configuration loaded", style("✓").green());
            let model = c.provider.openrouter.as_ref()
                .map(|o| o.default_model.as_str())
                .unwrap_or("not configured");
            println!("      └─ Model: {}", style(model).cyan());
            println!("      └─ Execution: {}", style(&c.sandbox.execution_env).cyan());
            Some(c)
        }
        Err(e) => {
            println!("   {} Configuration: {}", style("✗").red(), e);
            None
        }
    };

    if let Some(config) = config {
        // Check OpenRouter
        print!("   {} OpenRouter... ", style("○").dim());
        io::stdout().flush()?;
        match test_openrouter(&config).await {
            Ok(_) => println!("{}", style("✓ Connected").green()),
            Err(e) => println!("{} {}", style("✗").red(), e),
        }

        // Check Database
        print!("   {} PostgreSQL... ", style("○").dim());
        io::stdout().flush()?;
        match test_database(&config).await {
            Ok(_) => println!("{}", style("✓ Connected").green()),
            Err(e) => println!("{} {}", style("✗").red(), e),
        }

        // Check OpenSearch
        print!("   {} OpenSearch... ", style("○").dim());
        io::stdout().flush()?;
        match test_opensearch(&config).await {
            Ok(_) => println!("{}", style("✓ Connected").green()),
            Err(e) => println!("{} {}", style("✗").red(), e),
        }

        // Check Sandbox
        print!("   {} Sandbox ({})... ", style("○").dim(), config.sandbox.execution_env);
        io::stdout().flush()?;
        match test_sandbox(&config).await {
            Ok(_) => println!("{}", style("✓ Ready").green()),
            Err(e) => println!("{} {}", style("✗").red(), e),
        }

        // Check Telegram
        print!("   {} Telegram Bot... ", style("○").dim());
        io::stdout().flush()?;
        match test_telegram(&config).await {
            Ok(name) => println!("{} @{}", style("✓").green(), name),
            Err(e) => println!("{} {}", style("✗").red(), e),
        }
    }

    println!();

    // Offer quick fix options
    let options = &[
        "🔙 Return to main menu",
        "🔧 Run setup wizard to fix issues",
        "🔄 Retry connection tests",
    ];

    let choice = prompt_menu("Options:", options, 0)?;

    match choice {
        1 => {
            onboard(false).await?;
        }
        2 => {
            // Recursive call to retry
            Box::pin(check_status_interactive()).await?;
        }
        _ => {}
    }

    Ok(())
}

// ============================================================================
// Init Command
// ============================================================================

/// Initialize .env file with auto-configured values
fn init_env(force: bool) -> Result<()> {
    println!("🔧 OpenAgent Environment Initialization\n");

    let env_path = Path::new(".env");

    // Check if .env already exists
    if env_path.exists() && !force {
        println!("⚠️  .env file already exists.");
        println!("   Use --force to overwrite, or run 'openagent onboard' to verify.\n");

        // Show current port configuration
        if let Ok(vars) = read_env_file(env_path) {
            if let Some(port) = vars.get("WEBHOOK_PORT") {
                let port_num: u16 = port.parse().unwrap_or(0);
                let status = if is_port_available(port_num) { "✅ available" } else { "❌ in use" };
                println!("   Current WEBHOOK_PORT: {} ({})", port, status);
            }
        }
        return Ok(());
    }

    // Check if template exists
    if !Path::new(".env.example").exists() {
        println!("❌ .env.example not found.");
        println!("   Make sure you're in the OpenAgent project directory.");
        return Err(Error::Config("Missing .env.example".to_string()));
    }

    // Find free port
    print!("Finding available port... ");
    io::stdout().flush()?;

    let port = match find_free_port() {
        Some(p) => {
            println!("✅ {}", p);
            p
        }
        None => {
            println!("❌");
            return Err(Error::Config("No free port in range 20000-29999".to_string()));
        }
    };

    // Create .env file
    print!("Creating .env file... ");
    io::stdout().flush()?;

    create_env_file(port)?;
    println!("✅");

    println!("\n{}", "=".repeat(50));
    println!("✅ Environment initialized!");
    println!("{}", "=".repeat(50));

    println!("\n📝 Created .env with:");
    println!("   WEBHOOK_PORT={}", port);
    println!("   WEBHOOK_HOST=0.0.0.0");
    println!("   USE_LONG_POLLING=true");

    println!("\n⚠️  Required: Edit .env and add your API keys:");
    println!("   OPENROUTER_API_KEY=your_key_here");
    println!("   TELEGRAM_BOT_TOKEN=your_token_here");

    println!("\n🚀 Next steps:");
    println!("   1. Edit .env with your API keys");
    println!("   2. Run: openagent onboard");
    println!("   3. Run: pnpm dev");

    Ok(())
}

// ============================================================================
// Onboarding Wizard
// ============================================================================

/// Interactive onboarding wizard
async fn onboard(install_daemon: bool) -> Result<()> {
    println!();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║       🚀 Welcome to OpenAgent Setup Wizard       ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();
    println!("This interactive wizard will guide you through");
    println!("configuring OpenAgent step by step.");
    println!();

    let env_path = Path::new(".env");
    let mut env_vars: HashMap<String, String> = if env_path.exists() {
        read_env_file(env_path)?
    } else {
        HashMap::new()
    };

    // Check if template exists
    if !Path::new(".env.example").exists() {
        println!("❌ .env.example not found.");
        println!("   Make sure you're in the OpenAgent project directory.");
        return Err(Error::Config("Missing .env.example".to_string()));
    }

    let total_steps = 5;

    // =========================================================================
    // Step 1: Port Configuration
    // =========================================================================
    print_step(1, total_steps, "Network Configuration");

    print!("   Finding available port... ");
    io::stdout().flush()?;

    let port = match find_free_port() {
        Some(p) => {
            println!("✅ Found port {}", p);
            p
        }
        None => {
            println!("❌");
            return Err(Error::Config("No free port in range 20000-29999".to_string()));
        }
    };

    // Check if there's an existing port configuration
    if let Some(existing_port) = env_vars.get("WEBHOOK_PORT") {
        let existing: u16 = existing_port.parse().unwrap_or(0);
        if existing > 0 && is_port_available(existing) {
            println!("   Current port {} is available.", existing);
            if !prompt_yes_no("   Keep existing port?", true)? {
                env_vars.insert("WEBHOOK_PORT".to_string(), port.to_string());
                println!("   ✅ Updated to port {}", port);
            }
        } else {
            println!("   ⚠️  Port {} is in use, switching to {}", existing, port);
            env_vars.insert("WEBHOOK_PORT".to_string(), port.to_string());
        }
    } else {
        env_vars.insert("WEBHOOK_PORT".to_string(), port.to_string());
        println!("   ✅ Configured port {}", port);
    }

    env_vars.insert("WEBHOOK_HOST".to_string(), "0.0.0.0".to_string());
    env_vars.insert("USE_LONG_POLLING".to_string(), "true".to_string());

    // =========================================================================
    // Step 2: OpenRouter API Key
    // =========================================================================
    print_step(2, total_steps, "OpenRouter Configuration");

    println!();
    println!("   OpenRouter provides access to multiple AI models (Claude, GPT-4, etc.)");
    println!("   Get your API key at: https://openrouter.ai/keys");
    println!();

    let current_key = env_vars.get("OPENROUTER_API_KEY").cloned().unwrap_or_default();
    let has_key = !current_key.is_empty() && current_key != "your_openrouter_api_key_here";

    let openrouter_key = if has_key {
        let masked = format!("{}...{}", &current_key[..8.min(current_key.len())],
                            &current_key[current_key.len().saturating_sub(4)..]);
        println!("   Current API key: {}", masked);
        if prompt_yes_no("   Keep existing API key?", true)? {
            current_key
        } else {
            prompt("   Enter new OpenRouter API key: ")?
        }
    } else {
        prompt("   Enter your OpenRouter API key: ")?
    };

    if openrouter_key.is_empty() {
        println!("   ⚠️  No API key provided. You'll need to add it to .env manually.");
    } else {
        env_vars.insert("OPENROUTER_API_KEY".to_string(), openrouter_key.clone());
        println!("   ✅ API key configured");
    }

    // Select default model
    let current_model = env_vars.get("DEFAULT_MODEL").cloned().unwrap_or_default();
    let has_custom_model = !current_model.is_empty() 
        && current_model != "anthropic/claude-3.5-sonnet"
        && current_model != "anthropic/claude-3-opus"
        && current_model != "openai/gpt-4-turbo"
        && current_model != "meta-llama/llama-3.1-70b-instruct"
        && current_model != "google/gemini-pro-1.5";

    // Build model options dynamically to include current custom model
    let model_options: Vec<String> = if has_custom_model {
        vec![
            format!("{}  ← Current", current_model),
            "anthropic/claude-3.5-sonnet  ← Recommended (best balance)".to_string(),
            "anthropic/claude-3-opus      ← Most capable".to_string(),
            "openai/gpt-4-turbo           ← OpenAI's latest".to_string(),
            "meta-llama/llama-3.1-70b     ← Open source".to_string(),
            "google/gemini-pro-1.5        ← Google's model".to_string(),
            "✏️  Custom (enter your own model ID)".to_string(),
        ]
    } else {
        vec![
            "anthropic/claude-3.5-sonnet  ← Recommended (best balance)".to_string(),
            "anthropic/claude-3-opus      ← Most capable".to_string(),
            "openai/gpt-4-turbo           ← OpenAI's latest".to_string(),
            "meta-llama/llama-3.1-70b     ← Open source".to_string(),
            "google/gemini-pro-1.5        ← Google's model".to_string(),
            "✏️  Custom (enter your own model ID)".to_string(),
        ]
    };

    let model_options_refs: Vec<&str> = model_options.iter().map(|s| s.as_str()).collect();
    let model_choice = prompt_menu("Select default AI model:", &model_options_refs, 0)?;

    let default_model = if has_custom_model {
        match model_choice {
            0 => current_model.clone(), // Keep current custom model
            1 => "anthropic/claude-3.5-sonnet".to_string(),
            2 => "anthropic/claude-3-opus".to_string(),
            3 => "openai/gpt-4-turbo".to_string(),
            4 => "meta-llama/llama-3.1-70b-instruct".to_string(),
            5 => "google/gemini-pro-1.5".to_string(),
            6 => {
                println!();
                println!("   Browse available models at: {}", style("https://openrouter.ai/models").cyan().underlined());
                println!("   Format: provider/model-name (e.g., anthropic/claude-3-haiku)");
                let custom = prompt("Enter model ID")?;
                if custom.is_empty() {
                    println!("   Using current: {}", current_model);
                    current_model.clone()
                } else {
                    custom
                }
            }
            _ => current_model.clone(),
        }
    } else {
        match model_choice {
            0 => "anthropic/claude-3.5-sonnet".to_string(),
            1 => "anthropic/claude-3-opus".to_string(),
            2 => "openai/gpt-4-turbo".to_string(),
            3 => "meta-llama/llama-3.1-70b-instruct".to_string(),
            4 => "google/gemini-pro-1.5".to_string(),
            5 => {
                println!();
                println!("   Browse available models at: {}", style("https://openrouter.ai/models").cyan().underlined());
                println!("   Format: provider/model-name (e.g., anthropic/claude-3-haiku)");
                let custom = prompt("Enter model ID")?;
                if custom.is_empty() {
                    println!("   Using default: anthropic/claude-3.5-sonnet");
                    "anthropic/claude-3.5-sonnet".to_string()
                } else {
                    custom
                }
            }
            _ => "anthropic/claude-3.5-sonnet".to_string(),
        }
    };
    env_vars.insert("DEFAULT_MODEL".to_string(), default_model.clone());
    println!("   {} Model set to {}", style("✓").green(), style(&default_model).cyan());

    // =========================================================================
    // Step 3: Telegram Bot Token
    // =========================================================================
    print_step(3, total_steps, "Telegram Bot Configuration");

    println!();
    println!("   Create a Telegram bot to chat with your agent:");
    println!("   1. Open Telegram and search for @BotFather");
    println!("   2. Send /newbot and follow the instructions");
    println!("   3. Copy the HTTP API token you receive");
    println!();

    let current_token = env_vars.get("TELEGRAM_BOT_TOKEN").cloned().unwrap_or_default();
    let has_token = !current_token.is_empty() && current_token != "your_telegram_bot_token_here";

    let telegram_token = if has_token {
        let masked = format!("{}...{}", &current_token[..8.min(current_token.len())],
                            &current_token[current_token.len().saturating_sub(4)..]);
        println!("   Current bot token: {}", masked);
        if prompt_yes_no("   Keep existing token?", true)? {
            current_token
        } else {
            prompt("   Enter new Telegram bot token: ")?
        }
    } else {
        prompt("   Enter your Telegram bot token: ")?
    };

    if telegram_token.is_empty() {
        println!("   ⚠️  No token provided. You'll need to add it to .env manually.");
    } else {
        env_vars.insert("TELEGRAM_BOT_TOKEN".to_string(), telegram_token.clone());
        println!("   ✅ Bot token configured");
    }

    // Optional: Restrict to specific users
    println!();
    let current_allowed = env_vars.get("ALLOWED_USERS")
        .or_else(|| env_vars.get("TELEGRAM_ALLOWED_USERS"))
        .cloned()
        .unwrap_or_default();
    let has_allowed_users = !current_allowed.is_empty();

    if has_allowed_users {
        println!("   Current allowed user IDs: {}", style(&current_allowed).cyan());
        if prompt_yes_no("   Keep existing user restrictions?", true)? {
            env_vars.insert("ALLOWED_USERS".to_string(), current_allowed.clone());
        } else if prompt_yes_no("   Configure new user restrictions?", true)? {
            println!();
            println!("   To find your Telegram user ID:");
            println!("   - Send a message to @userinfobot on Telegram");
            println!("   - It will reply with your user ID");
            println!();
            let user_ids = prompt("   Enter comma-separated user IDs (e.g., 123456789,987654321): ")?;
            if !user_ids.is_empty() {
                env_vars.insert("ALLOWED_USERS".to_string(), user_ids);
                println!("   ✅ User restrictions configured");
            } else {
                println!("   ⚠️  No restrictions configured. Bot will accept messages from anyone.");
            }
        } else {
            println!("   ⚠️  Restrictions removed. Bot will accept messages from anyone.");
        }
    } else if prompt_yes_no("   Restrict bot to specific Telegram user IDs? (recommended for security)", false)? {
        println!();
        println!("   To find your Telegram user ID:");
        println!("   - Send a message to @userinfobot on Telegram");
        println!("   - It will reply with your user ID");
        println!();
        let user_ids = prompt("   Enter comma-separated user IDs (e.g., 123456789,987654321): ")?;
        if !user_ids.is_empty() {
            env_vars.insert("ALLOWED_USERS".to_string(), user_ids);
            println!("   ✅ User restrictions configured");
        }
    }

    // =========================================================================
    // Step 4: Database Configuration (Optional)
    // =========================================================================
    print_step(4, total_steps, "Database Configuration (Optional)");

    println!();
    println!("   PostgreSQL enables long-term memory and conversation history.");
    println!("   OpenSearch enables full-text search across conversations.");
    println!();

    // Check if databases are already configured via environment (e.g., running in Docker)
    let env_database_url = std::env::var("DATABASE_URL").ok();
    let env_opensearch_url = std::env::var("OPENSEARCH_URL").ok();

    let postgres_started = if env_database_url.is_some() && env_opensearch_url.is_some() {
        println!("   {} Databases pre-configured via environment variables", style("✅").green());
        if let Some(ref db_url) = env_database_url {
            // Mask the password in the URL for display
            let masked = db_url.split('@').last().unwrap_or(db_url);
            println!("   PostgreSQL: ...@{}", masked);
            env_vars.insert("DATABASE_URL".to_string(), db_url.clone());
        }
        if let Some(ref os_url) = env_opensearch_url {
            println!("   OpenSearch: {}", os_url);
            env_vars.insert("OPENSEARCH_URL".to_string(), os_url.clone());
        }
        println!();
        println!("   Skipping database configuration (already set).");
        true // Databases are configured
    } else {
    // Check if Docker is available
    let docker_available = is_docker_available();
    if docker_available {
        println!("   {} Docker detected - can auto-start databases", style("🐳").bold());
        println!();
    }

    let db_options = if docker_available {
        vec![
            "🐳 Auto-start PostgreSQL with Docker (recommended)",
            "⚙️  Configure existing PostgreSQL manually",
            "⏭️  Skip PostgreSQL for now",
        ]
    } else {
        vec![
            "⚙️  Configure existing PostgreSQL manually",
            "⏭️  Skip PostgreSQL for now",
        ]
    };

    let db_choice = prompt_menu("PostgreSQL setup:", &db_options.iter().map(|s| *s).collect::<Vec<_>>(), 0)?;

    let pg_started = if docker_available {
        match db_choice {
            0 => {
                // Auto-start with Docker
                println!();
                match start_postgres_docker() {
                    Ok(_) => {
                        env_vars.insert("DATABASE_URL".to_string(), 
                            "postgres://openagent:openagent@localhost:5432/openagent".to_string());
                        println!("   ✅ PostgreSQL started and configured");
                        true
                    }
                    Err(e) => {
                        println!("   ❌ Failed to start PostgreSQL: {}", e);
                        println!("   You can configure it manually later.");
                        false
                    }
                }
            }
            1 => {
                // Manual configuration
                configure_postgres_manually(&mut env_vars)?
            }
            _ => {
                println!("   ⏭️  Skipping PostgreSQL (can be configured later)");
                false
            }
        }
    } else {
        match db_choice {
            0 => configure_postgres_manually(&mut env_vars)?,
            _ => {
                println!("   ⏭️  Skipping PostgreSQL (can be configured later)");
                false
            }
        }
    };

    // OpenSearch configuration
    println!();
    
    let os_options = if docker_available {
        vec![
            "🐳 Auto-start OpenSearch with Docker (recommended)",
            "⚙️  Configure existing OpenSearch manually",
            "⏭️  Skip OpenSearch for now",
        ]
    } else {
        vec![
            "⚙️  Configure existing OpenSearch manually",
            "⏭️  Skip OpenSearch for now",
        ]
    };

    let os_choice = prompt_menu("OpenSearch setup:", &os_options.iter().map(|s| *s).collect::<Vec<_>>(), 0)?;

    let _opensearch_started = if docker_available {
        match os_choice {
            0 => {
                // Auto-start with Docker
                println!();
                match start_opensearch_docker() {
                    Ok(_) => {
                        env_vars.insert("OPENSEARCH_URL".to_string(), 
                            "http://localhost:9200".to_string());
                        println!("   ✅ OpenSearch started and configured");
                        true
                    }
                    Err(e) => {
                        println!("   ❌ Failed to start OpenSearch: {}", e);
                        println!("   You can configure it manually later.");
                        false
                    }
                }
            }
            1 => {
                // Manual configuration
                configure_opensearch_manually(&mut env_vars)?
            }
            _ => {
                println!("   ⏭️  Skipping OpenSearch (can be configured later)");
                false
            }
        }
    } else {
        match os_choice {
            0 => configure_opensearch_manually(&mut env_vars)?,
            _ => {
                println!("   ⏭️  Skipping OpenSearch (can be configured later)");
                false
            }
        }
    };
    pg_started // return postgres status from else block
    }; // end of else block for pre-configured databases

    // Run migrations if PostgreSQL was configured
    if postgres_started {
        println!();
        if prompt_yes_no("   Run database migrations now?", true)? {
            print!("   Running migrations... ");
            io::stdout().flush()?;
            // Save config first so migrations can read it (skip if running in Docker with pre-configured DBs)
            let databases_preconfigured = env_database_url.is_some() && env_opensearch_url.is_some();
            if !databases_preconfigured {
                write_env_file(env_path, &env_vars)?;
                dotenvy::from_path(env_path).ok();
            }

            match run_migrations_internal().await {
                Ok(_) => println!("✅"),
                Err(e) => println!("❌ {}", e),
            }
        }
    }

    // =========================================================================
    // Step 5: Execution Environment
    // =========================================================================
    print_step(5, total_steps, "Sandbox Configuration");

    println!();
    println!("   Choose how the agent executes code:");

    let sandbox_options = &[
        "OS Mode       ← Recommended (simple, runs in restricted directory)",
        "Sandbox Mode  ← WebAssembly isolation (more secure)",
        "Container     ← Docker containers (most secure, requires Docker)",
    ];

    let sandbox_choice = prompt_menu("Execution environment:", sandbox_options, 0)?;

    let execution_env = match sandbox_choice {
        0 => "os",
        1 => "sandbox",
        2 => "container",
        _ => "os",
    };
    env_vars.insert("EXECUTION_ENV".to_string(), execution_env.to_string());

    if execution_env == "container" {
        let image = prompt_with_default("   Docker image", "python:3.11-slim")?;
        env_vars.insert("CONTAINER_IMAGE".to_string(), image);
    }

    // Generate a unique workspace directory with UUID
    let current_workspace = env_vars.get("ALLOWED_DIR").cloned().unwrap_or_default();
    let default_workspace = if current_workspace.is_empty() || current_workspace == "/tmp/openagent-workspace" {
        let uuid = uuid::Uuid::new_v4();
        format!("/tmp/openagent-ws-{}", uuid)
    } else {
        current_workspace
    };
    let workspace = prompt_with_default("   Workspace directory", &default_workspace)?;
    env_vars.insert("ALLOWED_DIR".to_string(), workspace);

    println!("   ✅ Sandbox configured ({})", execution_env);

    // =========================================================================
    // Save Configuration
    // =========================================================================
    print_section("Saving Configuration");

    // Always try to save the .env file (needed for Telegram token, etc.)
    let databases_preconfigured = env_database_url.is_some() && env_opensearch_url.is_some();
    let mut save_succeeded = false;
    match write_env_file(env_path, &env_vars) {
        Ok(_) => {
            // Verify the write actually persisted by re-reading the file
            match read_env_file(env_path) {
                Ok(saved_vars) => {
                    // Check critical values were saved
                    let telegram_saved = saved_vars.get("TELEGRAM_BOT_TOKEN") == env_vars.get("TELEGRAM_BOT_TOKEN");
                    let openrouter_saved = saved_vars.get("OPENROUTER_API_KEY") == env_vars.get("OPENROUTER_API_KEY");

                    if telegram_saved && openrouter_saved {
                        println!("   ✅ Configuration saved to .env");
                        save_succeeded = true;
                    } else {
                        println!("   ⚠️  Configuration may not have been saved correctly");
                        if !telegram_saved && !telegram_token.is_empty() {
                            println!("   ⚠️  Telegram token was not persisted");
                        }
                        if !openrouter_saved && !openrouter_key.is_empty() {
                            println!("   ⚠️  OpenRouter API key was not persisted");
                        }
                        if databases_preconfigured {
                            println!("   ℹ️  Edit .env on the host machine to add your API keys");
                        }
                    }
                }
                Err(_) => {
                    println!("   ⚠️  Could not verify saved configuration");
                    save_succeeded = true; // Assume it worked
                }
            }
        }
        Err(e) => {
            if databases_preconfigured {
                // In Docker with read-only .env, this is expected
                println!("   ℹ️  Could not save .env (read-only): {}", e);
                println!("   ⚠️  Edit .env on the host to configure Telegram token");
            } else {
                return Err(e);
            }
        }
    }

    // If save didn't succeed in Docker, show the values to copy manually
    if !save_succeeded && databases_preconfigured {
        println!();
        println!("   To configure manually, add these to your .env file on the host:");
        if !telegram_token.is_empty() {
            println!("   TELEGRAM_BOT_TOKEN={}", telegram_token);
        }
        if !openrouter_key.is_empty() {
            let masked = format!("{}...{}", &openrouter_key[..8.min(openrouter_key.len())],
                                &openrouter_key[openrouter_key.len().saturating_sub(4)..]);
            println!("   OPENROUTER_API_KEY={} (masked)", masked);
        }
        println!();
    }

    // =========================================================================
    // Verify Configuration
    // =========================================================================
    print_section("Verifying Configuration");

    // Always reload env vars for verification to pick up newly saved values
    // Use override mode to ensure new values take precedence over existing env vars
    dotenvy::from_path_override(env_path).ok();

    // Also explicitly set env vars from our collected values to ensure verification works
    // This handles cases where the .env file write didn't persist (e.g., Docker bind mount issues)
    if !telegram_token.is_empty() {
        std::env::set_var("TELEGRAM_BOT_TOKEN", &telegram_token);
    }
    if !openrouter_key.is_empty() {
        std::env::set_var("OPENROUTER_API_KEY", &openrouter_key);
    }

    // Test OpenRouter
    print!("   Checking OpenRouter... ");
    io::stdout().flush()?;
    if !openrouter_key.is_empty() {
        match Config::from_env() {
            Ok(config) => match test_openrouter(&config).await {
                Ok(_) => println!("✅ Connected"),
                Err(e) => println!("❌ {}", e),
            },
            Err(e) => println!("❌ Config error: {}", e),
        }
    } else {
        println!("⏭️  Skipped (no API key)");
    }

    // Test Telegram
    print!("   Checking Telegram bot... ");
    io::stdout().flush()?;
    if !telegram_token.is_empty() {
        match Config::from_env() {
            Ok(config) => match test_telegram(&config).await {
                Ok(name) => println!("✅ @{}", name),
                Err(e) => println!("❌ {}", e),
            },
            Err(e) => println!("❌ Config error: {}", e),
        }
    } else {
        println!("⏭️  Skipped (no token)");
    }

    // Test Database (if configured)
    if env_vars.contains_key("DATABASE_URL") {
        print!("   Checking PostgreSQL... ");
        io::stdout().flush()?;
        match Config::from_env() {
            Ok(config) => match test_database(&config).await {
                Ok(_) => println!("✅ Connected"),
                Err(e) => println!("❌ {}", e),
            },
            Err(e) => println!("❌ Config error: {}", e),
        }
    }

    // Install daemon if requested
    if install_daemon {
        print_section("Installing Systemd Service");
        install_systemd_service()?;
    }

    // =========================================================================
    // Summary
    // =========================================================================
    println!();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║           ✅ Setup Complete!                     ║");
    println!("╚══════════════════════════════════════════════════╝");

    let final_port = env_vars.get("WEBHOOK_PORT").map(|s| s.as_str()).unwrap_or("20000");

    println!();
    println!("📋 Configuration Summary:");
    println!("   • Webhook Port: {}", final_port);
    println!("   • AI Model: {}", default_model);
    println!("   • Execution: {}", execution_env);
    println!("   • Workspace: {}", env_vars.get("ALLOWED_DIR").map(|s| s.as_str()).unwrap_or("/tmp/openagent-workspace"));

    println!();
    println!("🚀 Next Steps:");
    println!("   1. Build the project:");
    println!("      cargo build --release");
    println!();
    println!("   2. Start the Telegram gateway:");
    println!("      cargo run --release --bin gateway");
    println!("      OR: pnpm dev");
    println!();
    println!("   3. Open Telegram and chat with your bot!");
    println!();

    if openrouter_key.is_empty() || telegram_token.is_empty() {
        println!("⚠️  Remember to edit .env and add missing API keys!");
        println!();
    }

    Ok(())
}

async fn test_openrouter(config: &Config) -> Result<()> {
    use openagent::agent::OpenRouterClient;

    let openrouter_config = config.provider.openrouter.clone()
        .ok_or_else(|| Error::Config("OpenRouter not configured".into()))?;
    let client = OpenRouterClient::new(openrouter_config)?;
    client.list_models().await?;
    Ok(())
}

async fn test_database(config: &Config) -> Result<()> {
    let postgres = config.storage.postgres.as_ref()
        .ok_or_else(|| Error::Config("PostgreSQL not configured".into()))?;
    let pool = init_pool(postgres).await?;
    sqlx::query("SELECT 1").execute(&pool).await?;
    Ok(())
}

async fn test_opensearch(config: &Config) -> Result<()> {
    let os_config = config.storage.opensearch.as_ref()
        .ok_or_else(|| Error::Config("OpenSearch not configured".into()))?;
    let client = OpenSearchClient::new(os_config).await?;
    client.health_check().await?;
    Ok(())
}

async fn test_sandbox(config: &Config) -> Result<()> {
    use openagent::sandbox::{create_executor, ExecutionRequest, Language};

    let executor = create_executor(&config.sandbox).await?;

    // Only run actual test for OS mode (Wasm and Container might need setup)
    if config.sandbox.execution_env == ExecutionEnv::Os {
        let request = ExecutionRequest::new("print('test')", Language::Python);
        let result = executor.execute(request).await?;
        if !result.success {
            return Err(Error::Sandbox("Test execution failed".to_string()));
        }
    }

    Ok(())
}

async fn test_telegram(config: &Config) -> Result<String> {
    use teloxide::prelude::*;
    use secrecy::ExposeSecret;

    let telegram_config = config.channels.telegram.as_ref()
        .ok_or_else(|| Error::Config("Telegram not configured".into()))?;
    let bot = Bot::new(telegram_config.bot_token.expose_secret());
    let me = bot.get_me().await.map_err(|e| Error::Telegram(e.to_string()))?;
    Ok(me.username.clone().unwrap_or_else(|| "unknown".to_string()))
}

// ============================================================================
// Docker Management
// ============================================================================

/// Check if Docker is available
fn is_docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if a Docker container is running
fn is_container_running(name: &str) -> bool {
    std::process::Command::new("docker")
        .args(["ps", "-q", "-f", &format!("name={}", name)])
        .output()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false)
}

/// Check if a Docker container exists (running or stopped)
fn container_exists(name: &str) -> bool {
    std::process::Command::new("docker")
        .args(["ps", "-aq", "-f", &format!("name={}", name)])
        .output()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false)
}

/// Start PostgreSQL using Docker
fn start_postgres_docker() -> Result<()> {
    const CONTAINER_NAME: &str = "openagent-postgres";

    if is_container_running(CONTAINER_NAME) {
        println!("   {} PostgreSQL container already running", style("ℹ").blue());
        return Ok(());
    }

    if container_exists(CONTAINER_NAME) {
        // Container exists but stopped, start it
        print!("   Starting existing PostgreSQL container... ");
        io::stdout().flush()?;
        
        let status = std::process::Command::new("docker")
            .args(["start", CONTAINER_NAME])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|e| Error::Config(format!("Failed to start container: {}", e)))?;

        if status.success() {
            println!("✅");
            // Wait for PostgreSQL to be ready
            wait_for_postgres()?;
            return Ok(());
        } else {
            println!("❌");
            return Err(Error::Config("Failed to start existing container".to_string()));
        }
    }

    // Create new container
    print!("   Creating PostgreSQL container... ");
    io::stdout().flush()?;

    let status = std::process::Command::new("docker")
        .args([
            "run", "-d",
            "--name", CONTAINER_NAME,
            "-e", "POSTGRES_USER=openagent",
            "-e", "POSTGRES_PASSWORD=openagent",
            "-e", "POSTGRES_DB=openagent",
            "-p", "5432:5432",
            "--restart", "unless-stopped",
            "pgvector/pgvector:pg16"
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status()
        .map_err(|e| Error::Config(format!("Failed to run docker: {}", e)))?;

    if status.success() {
        println!("✅");
        // Wait for PostgreSQL to be ready
        wait_for_postgres()?;
        Ok(())
    } else {
        println!("❌");
        Err(Error::Config("Failed to create PostgreSQL container".to_string()))
    }
}

/// Wait for PostgreSQL to be ready
fn wait_for_postgres() -> Result<()> {
    print!("   Waiting for PostgreSQL to be ready... ");
    io::stdout().flush()?;

    for i in 0..30 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        
        let status = std::process::Command::new("docker")
            .args(["exec", "openagent-postgres", "pg_isready", "-U", "openagent"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        if status.map(|s| s.success()).unwrap_or(false) {
            println!("✅ ({}s)", i + 1);
            return Ok(());
        }
    }

    println!("⚠️  Timeout");
    Ok(()) // Continue anyway, might work
}

/// Start OpenSearch using Docker
fn start_opensearch_docker() -> Result<()> {
    const CONTAINER_NAME: &str = "openagent-opensearch";

    if is_container_running(CONTAINER_NAME) {
        println!("   {} OpenSearch container already running", style("ℹ").blue());
        return Ok(());
    }

    if container_exists(CONTAINER_NAME) {
        // Container exists but stopped, start it
        print!("   Starting existing OpenSearch container... ");
        io::stdout().flush()?;
        
        let status = std::process::Command::new("docker")
            .args(["start", CONTAINER_NAME])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|e| Error::Config(format!("Failed to start container: {}", e)))?;

        if status.success() {
            println!("✅");
            wait_for_opensearch()?;
            return Ok(());
        } else {
            println!("❌");
            return Err(Error::Config("Failed to start existing container".to_string()));
        }
    }

    // Create new container
    print!("   Creating OpenSearch container... ");
    io::stdout().flush()?;

    let status = std::process::Command::new("docker")
        .args([
            "run", "-d",
            "--name", CONTAINER_NAME,
            "-e", "discovery.type=single-node",
            "-e", "DISABLE_SECURITY_PLUGIN=true",
            "-e", "OPENSEARCH_INITIAL_ADMIN_PASSWORD=OpenAgent123!",
            "-p", "9200:9200",
            "--restart", "unless-stopped",
            "opensearchproject/opensearch:2"
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status()
        .map_err(|e| Error::Config(format!("Failed to run docker: {}", e)))?;

    if status.success() {
        println!("✅");
        wait_for_opensearch()?;
        Ok(())
    } else {
        println!("❌");
        Err(Error::Config("Failed to create OpenSearch container".to_string()))
    }
}

/// Wait for OpenSearch to be ready
fn wait_for_opensearch() -> Result<()> {
    print!("   Waiting for OpenSearch to be ready... ");
    io::stdout().flush()?;

    for i in 0..60 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        
        // Try to connect to OpenSearch
        let status = std::process::Command::new("curl")
            .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", "http://localhost:9200"])
            .output();

        if let Ok(output) = status {
            if output.stdout == b"200" {
                println!("✅ ({}s)", i + 1);
                return Ok(());
            }
        }
    }

    println!("⚠️  Timeout (may still be starting)");
    Ok(()) // Continue anyway
}

/// Manual PostgreSQL configuration
fn configure_postgres_manually(env_vars: &mut HashMap<String, String>) -> Result<bool> {
    let db_host = prompt_with_default("   Database host", "localhost")?;
    let db_port = prompt_with_default("   Database port", "5432")?;
    let db_name = prompt_with_default("   Database name", "openagent")?;
    let db_user = prompt_with_default("   Database user", "openagent")?;
    let db_pass = prompt("   Database password: ")?;

    let db_url = format!(
        "postgres://{}:{}@{}:{}/{}",
        db_user, db_pass, db_host, db_port, db_name
    );
    env_vars.insert("DATABASE_URL".to_string(), db_url);
    println!("   ✅ PostgreSQL configured");
    Ok(true)
}

/// Manual OpenSearch configuration
fn configure_opensearch_manually(env_vars: &mut HashMap<String, String>) -> Result<bool> {
    let os_url = prompt_with_default("   OpenSearch URL", "http://localhost:9200")?;
    env_vars.insert("OPENSEARCH_URL".to_string(), os_url);

    if prompt_yes_no("   Does OpenSearch require authentication?", false)? {
        let os_user = prompt("   OpenSearch username: ")?;
        let os_pass = prompt("   OpenSearch password: ")?;
        env_vars.insert("OPENSEARCH_USERNAME".to_string(), os_user);
        env_vars.insert("OPENSEARCH_PASSWORD".to_string(), os_pass);
    }
    println!("   ✅ OpenSearch configured");
    Ok(true)
}

/// Run migrations (internal helper)
async fn run_migrations_internal() -> Result<()> {
    let config = Config::from_env()?;
    let postgres = config.storage.postgres.as_ref()
        .ok_or_else(|| Error::Config("PostgreSQL not configured for migrations".into()))?;
    // Use init_pool_for_migrations to skip pgvector check - migrations will create it
    let pool = init_pool_for_migrations(postgres).await?;
    migrations::run(&pool).await?;
    Ok(())
}

fn install_systemd_service() -> Result<()> {
    let service = r#"[Unit]
Description=OpenAgent Gateway
After=network.target postgresql.service

[Service]
Type=simple
WorkingDirectory=/path/to/openagent
ExecStart=/path/to/openagent-gateway
Restart=always
RestartSec=10
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
"#;

    println!("Sample systemd service file:");
    println!("{}", service);
    println!("\nTo install:");
    println!("1. Save this to /etc/systemd/system/openagent.service");
    println!("2. Update the paths");
    println!("3. Run: sudo systemctl daemon-reload");
    println!("4. Run: sudo systemctl enable --now openagent");

    Ok(())
}

/// Check status of all services
async fn check_status() -> Result<()> {
    println!("🔍 OpenAgent Status\n");

    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            println!("❌ Configuration: {}", e);
            return Ok(());
        }
    };

    println!("Configuration: ✅ Loaded");
    let default_model = config.provider.openrouter.as_ref()
        .map(|c| c.default_model.as_str())
        .unwrap_or("not configured");
    println!("  Model: {}", default_model);
    println!("  Execution: {}", config.sandbox.execution_env);

    // Check OpenRouter
    match test_openrouter(&config).await {
        Ok(_) => println!("OpenRouter: ✅ Connected"),
        Err(e) => println!("OpenRouter: ❌ {}", e),
    }

    // Check Database
    match test_database(&config).await {
        Ok(_) => println!("PostgreSQL: ✅ Connected"),
        Err(e) => println!("PostgreSQL: ❌ {}", e),
    }

    // Check OpenSearch
    match test_opensearch(&config).await {
        Ok(_) => println!("OpenSearch: ✅ Connected"),
        Err(e) => println!("OpenSearch: ❌ {}", e),
    }

    Ok(())
}

/// Run database migrations
async fn run_migrations() -> Result<()> {
    println!("Running database migrations...\n");

    let config = Config::from_env()?;
    let postgres = config.storage.postgres.as_ref()
        .ok_or_else(|| Error::Config("PostgreSQL not configured for migrations".into()))?;
    // Use init_pool_for_migrations to skip pgvector check - migrations will create it
    let pool = init_pool_for_migrations(postgres).await?;

    migrations::run(&pool).await?;

    // Initialize OpenSearch indexes if available
    if let Some(os_config) = &config.storage.opensearch {
        match OpenSearchClient::new(os_config).await {
            Ok(client) => {
                client.init_indexes().await?;
                println!("OpenSearch indexes initialized");
            }
            Err(e) => {
                println!("OpenSearch not available: {}", e);
            }
        }
    }

    println!("\n✅ Migrations complete!");
    Ok(())
}

/// Test LLM connection
async fn test_llm(model: Option<String>) -> Result<()> {
    use openagent::agent::{GenerationOptions, Message, OpenRouterClient};

    let config = Config::from_env()?;
    let openrouter_config = config.provider.openrouter.clone()
        .ok_or_else(|| Error::Config("OpenRouter not configured".into()))?;
    let client = OpenRouterClient::new(openrouter_config.clone())?;

    let model = model.unwrap_or(openrouter_config.default_model);
    println!("Testing model: {}\n", model);

    let messages = vec![
        Message::system("You are a helpful assistant. Keep responses brief."),
        Message::user("Say 'Hello from OpenAgent!' in exactly those words."),
    ];

    let response = client
        .chat_with_model(&model, messages, GenerationOptions::precise())
        .await?;

    if let Some(choice) = response.choices.first() {
        println!("Response: {}", choice.message.content);
    }

    if let Some(usage) = response.usage {
        println!("\nTokens used: {}", usage.total_tokens);
    }

    println!("\n✅ LLM test successful!");
    Ok(())
}

/// Run code in sandbox
async fn run_code(language: &str, code: &str) -> Result<()> {
    use openagent::sandbox::{create_executor, ExecutionRequest, Language};

    let config = Config::from_env()?;
    let executor = create_executor(&config.sandbox).await?;

    let language: Language = language.parse()?;
    let request = ExecutionRequest::new(code, language);

    println!("Executing {} code...\n", language);

    let result = executor.execute(request).await?;

    if result.success {
        println!("Output:\n{}", result.stdout);
    } else if result.timed_out {
        println!("❌ Execution timed out");
    } else {
        println!("❌ Execution failed:\n{}", result.stderr);
    }

    println!("\nTime: {:?}", result.execution_time);
    Ok(())
}

/// List available models with interactive selection
async fn list_models() -> Result<()> {
    list_models_interactive(false).await.map(|_| ())
}

/// Interactive model browser with fuzzy search
async fn list_models_interactive(select_mode: bool) -> Result<Option<String>> {
    use openagent::agent::OpenRouterClient;

    let config = Config::from_env()?;
    let openrouter_config = config.provider.openrouter
        .ok_or_else(|| Error::Config("OpenRouter not configured".into()))?;
    let client = OpenRouterClient::new(openrouter_config)?;

    println!("\n{}", style("Loading available models...").dim());

    let models = client.list_models().await?;

    // Clear the loading message
    let term = Term::stdout();
    let _ = term.clear_last_lines(1);

    // Build display strings for each model
    let model_displays: Vec<String> = models
        .iter()
        .map(|m| {
            let price: f64 = m.pricing.prompt.parse().unwrap_or(0.0) * 1000.0;
            format!(
                "{:<45} │ {:>6}k ctx │ ${:.4}/1k",
                m.id,
                m.context_length / 1000,
                price
            )
        })
        .collect();

    println!();
    println!("{}", style("╔══════════════════════════════════════════════════════════════════════════╗").cyan());
    println!("{}", style("║                    🤖 OpenRouter Model Browser                           ║").cyan());
    println!("{}", style("╚══════════════════════════════════════════════════════════════════════════╝").cyan());
    println!();
    println!("   {} models available", style(models.len()).green().bold());
    println!("   {}", style("Type to search, ↑/↓ to navigate, Enter to select").dim());
    println!();

    if select_mode {
        // Use FuzzySelect for searching
        let selection = FuzzySelect::with_theme(&theme())
            .with_prompt("Search models")
            .items(&model_displays)
            .default(0)
            .interact_opt()
            .map_err(|e| Error::Config(format!("Selection error: {}", e)))?;

        if let Some(idx) = selection {
            let model_id = models[idx].id.clone();
            println!("\n   {} Selected: {}", style("✓").green(), style(&model_id).cyan());
            return Ok(Some(model_id));
        }
        Ok(None)
    } else {
        // Browse mode - show options
        let browse_options = &[
            "🔍 Search and select a model",
            "📋 List popular models",
            "🔙 Back to menu",
        ];

        let choice = prompt_menu("What would you like to do?", browse_options, 0)?;

        match choice {
            0 => {
                // Search mode
                let selection = FuzzySelect::with_theme(&theme())
                    .with_prompt("Search models")
                    .items(&model_displays)
                    .default(0)
                    .interact_opt()
                    .map_err(|e| Error::Config(format!("Selection error: {}", e)))?;

                if let Some(idx) = selection {
                    let model = &models[idx];
                    println!();
                    println!("{}" , style("═".repeat(60)).dim());
                    println!("  {} {}", style("Model:").bold(), style(&model.id).cyan());
                    println!("  {} {}k tokens", style("Context:").bold(), model.context_length / 1000);
                    println!("  {} ${}/1k prompt, ${}/1k completion", 
                        style("Pricing:").bold(), model.pricing.prompt, model.pricing.completion);
                    if !model.description.is_empty() {
                        let desc: String = model.description.chars().take(100).collect();
                        println!("  {} {}", style("Description:").bold(), desc);
                    }
                    println!("{}" , style("═".repeat(60)).dim());

                    if prompt_yes_no("\nSet as default model?", false)? {
                        update_env_var("DEFAULT_MODEL", &model.id)?;
                        println!("   {} Default model updated to {}", style("✓").green(), style(&model.id).cyan());
                    }
                }
            }
            1 => {
                // List popular models
                let popular = [
                    "anthropic/claude-3.5-sonnet",
                    "anthropic/claude-3-opus",
                    "openai/gpt-4-turbo",
                    "openai/gpt-4o",
                    "meta-llama/llama-3.1-70b-instruct",
                    "google/gemini-pro-1.5",
                    "mistralai/mistral-large",
                ];

                println!("\n{}", style("Popular Models:").cyan().bold());
                for model_id in popular {
                    if let Some(model) = models.iter().find(|m| m.id == model_id) {
                        println!("   • {} ({}k context)", 
                            style(&model.id).green(),
                            model.context_length / 1000);
                    }
                }
            }
            _ => {}
        }

        Ok(None)
    }
}

/// Update a single environment variable in .env
fn update_env_var(key: &str, value: &str) -> Result<()> {
    let env_path = Path::new(".env");
    let mut vars = read_env_file(env_path)?;
    vars.insert(key.to_string(), value.to_string());
    write_env_file(env_path, &vars)
}

/// Generate sample configuration
fn init_config() -> Result<()> {
    let example = include_str!("../../.env.example");
    println!("{}", example);
    Ok(())
}

/// Interactive chat mode with model selection
async fn interactive_chat(model: Option<String>) -> Result<()> {
    use openagent::agent::{Conversation, GenerationOptions, OpenRouterClient};

    let config = Config::from_env()?;
    let openrouter_config = config.provider.openrouter.clone()
        .ok_or_else(|| Error::Config("OpenRouter not configured".into()))?;
    let client = OpenRouterClient::new(openrouter_config.clone())?;

    println!();
    println!("{}", style("╔══════════════════════════════════════════════════╗").cyan());
    println!("{}", style("║           🤖 OpenAgent Interactive Chat          ║").cyan());
    println!("{}", style("╚══════════════════════════════════════════════════╝").cyan());
    println!();

    // Model selection if not provided
    let model = if let Some(m) = model {
        m
    } else {
        let model_options = &[
            "Use default model",
            "Browse and select a model",
            "Enter model ID manually",
        ];

        let choice = prompt_menu("How would you like to select a model?", model_options, 0)?;

        match choice {
            0 => openrouter_config.default_model.clone(),
            1 => {
                if let Some(selected) = list_models_interactive(true).await? {
                    selected
                } else {
                    openrouter_config.default_model.clone()
                }
            }
            2 => {
                let custom = prompt("Enter model ID (e.g., anthropic/claude-3.5-sonnet)")?;
                if custom.is_empty() {
                    openrouter_config.default_model.clone()
                } else {
                    custom
                }
            }
            _ => openrouter_config.default_model.clone(),
        }
    };

    println!();
    println!("   {} Using model: {}", style("✓").green(), style(&model).cyan());
    println!();
    println!("   {}", style("Commands:").dim());
    println!("   {}  - Exit chat", style("/quit").yellow());
    println!("   {}  - Clear conversation history", style("/clear").yellow());
    println!("   {}  - Change model", style("/model").yellow());
    println!("   {} - View/edit agent soul", style("/soul").yellow());
    println!("   {}  - Show this help", style("/help").yellow());
    println!();

    // Load the agent's soul for personality
    let soul = openagent::agent::prompts::Soul::load_or_default();
    let system_prompt = soul.as_system_prompt();

    let mut conversation = Conversation::new("cli-user", &model)
        .with_system_prompt(&system_prompt);
    let mut current_model = model;

    loop {
        // Use dialoguer for better input experience
        let user_input: String = Input::with_theme(&theme())
            .with_prompt(style("You").green().bold().to_string())
            .allow_empty(true)
            .interact_text()
            .map_err(|e| Error::Config(format!("Input error: {}", e)))?;

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
                    conversation.clear();
                    let term = Term::stdout();
                    let _ = term.clear_screen();
                    println!("\n   {} Conversation cleared.\n", style("✓").green());
                    continue;
                }
                "/model" | "/m" => {
                    if let Some(new_model) = list_models_interactive(true).await? {
                        current_model = new_model.clone();
                        // Reload soul in case it was edited
                        let soul = openagent::agent::prompts::Soul::load_or_default();
                        conversation = Conversation::new("cli-user", &current_model)
                            .with_system_prompt(soul.as_system_prompt());
                        println!("\n   {} Switched to {}, conversation cleared.\n", 
                            style("✓").green(), style(&current_model).cyan());
                    }
                    continue;
                }
                "/soul" | "/s" => {
                    manage_soul(None)?;
                    // Reload the soul after editing
                    let soul = openagent::agent::prompts::Soul::load_or_default();
                    conversation = Conversation::new("cli-user", &current_model)
                        .with_system_prompt(soul.as_system_prompt());
                    println!("   {} Soul reloaded into conversation.\n", style("✓").green());
                    continue;
                }
                "/help" | "/h" | "/?" => {
                    println!();
                    println!("   {}", style("Available Commands:").cyan().bold());
                    println!("   {}  - Exit chat", style("/quit").yellow());
                    println!("   {}  - Clear conversation", style("/clear").yellow());
                    println!("   {}  - Change model", style("/model").yellow());
                    println!("   {} - View/edit soul", style("/soul").yellow());
                    println!("   {}  - Show help", style("/help").yellow());
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

        conversation.add_user_message(input);

        // Show typing indicator
        print!("   {} ", style("●●●").dim());
        io::stdout().flush()?;

        match client
            .chat_with_model(&current_model, conversation.get_api_messages(), GenerationOptions::balanced())
            .await
        {
            Ok(response) => {
                // Clear typing indicator
                let term = Term::stdout();
                let _ = term.clear_line();

                if let Some(choice) = response.choices.first() {
                    let reply = &choice.message.content;
                    conversation.add_assistant_message(reply);
                    println!("\n   {}: {}\n", style("Assistant").cyan().bold(), reply);
                }
            }
            Err(e) => {
                let term = Term::stdout();
                let _ = term.clear_line();
                println!("\n   {} Error: {}\n", style("❌").red(), e);
            }
        }
    }

    Ok(())
}

// ============================================================================
// Soul Management
// ============================================================================

/// Manage the agent's soul (personality configuration)
fn manage_soul(action: Option<SoulAction>) -> Result<()> {
    let action = action.unwrap_or_else(|| {
        // Interactive menu if no action specified
        let options = &[
            "👁️  View current soul",
            "✏️  Edit soul in editor",
            "🔄 Reset to default",
            "💡 Add learned preference",
            "🔙 Cancel",
        ];

        match prompt_menu("Soul Management:", options, 0).unwrap_or(4) {
            0 => SoulAction::View,
            1 => SoulAction::Edit,
            2 => SoulAction::Reset,
            3 => {
                let text = prompt("Enter preference to learn: ").unwrap_or_default();
                SoulAction::Learn { text }
            }
            _ => SoulAction::View,
        }
    });

    match action {
        SoulAction::View => view_soul(),
        SoulAction::Edit => edit_soul(),
        SoulAction::Reset => reset_soul(),
        SoulAction::Learn { text } => learn_preference(&text),
    }
}

/// View the current soul
fn view_soul() -> Result<()> {
    use openagent::agent::prompts::Soul;

    println!();
    println!("{}", style("╔══════════════════════════════════════════════════════════════╗").magenta());
    println!("{}", style("║                    🧠 Agent Soul                             ║").magenta());
    println!("{}", style("╚══════════════════════════════════════════════════════════════╝").magenta());
    println!();

    let soul = Soul::load_or_default();
    
    // Pretty print the soul content
    for line in soul.content.lines() {
        if line.starts_with("# ") {
            println!("{}", style(line).magenta().bold());
        } else if line.starts_with("## ") {
            println!("\n{}", style(line).cyan().bold());
        } else if line.starts_with("### ") {
            println!("{}", style(line).yellow());
        } else if line.starts_with("- ") || line.starts_with("* ") {
            println!("  {}", line);
        } else if line.starts_with("---") {
            println!("{}", style("─".repeat(60)).dim());
        } else {
            println!("{}", line);
        }
    }

    println!();
    println!("{}", style("─".repeat(60)).dim());
    println!("   {} Edit with: {} or {}", 
        style("💡").bold(),
        style("openagent soul edit").cyan(),
        style("pnpm openagent soul edit").cyan());
    println!();

    Ok(())
}

/// Edit the soul in the default editor
fn edit_soul() -> Result<()> {
    use openagent::agent::prompts::{Soul, SOUL_FILE_PATH};

    // Ensure SOUL.md exists
    if !Path::new(SOUL_FILE_PATH).exists() {
        let soul = Soul::default();
        soul.save()?;
        println!("   {} Created default SOUL.md", style("✓").green());
    }

    // Get the editor
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| {
            // Try common editors
            for editor in ["nano", "vim", "vi", "code", "notepad"] {
                if which::which(editor).is_ok() {
                    return editor.to_string();
                }
            }
            "nano".to_string()
        });

    println!();
    println!("   {} Opening SOUL.md in {}...", style("📝").bold(), style(&editor).cyan());
    println!();

    let status = std::process::Command::new(&editor)
        .arg(SOUL_FILE_PATH)
        .status()
        .map_err(|e| Error::Config(format!("Failed to open editor: {}", e)))?;

    if status.success() {
        println!("   {} Soul updated!", style("✓").green());
    } else {
        println!("   {} Editor exited with error", style("⚠").yellow());
    }

    Ok(())
}

/// Reset soul to default
fn reset_soul() -> Result<()> {
    use openagent::agent::prompts::Soul;

    println!();
    if prompt_yes_no("   Are you sure you want to reset SOUL.md to default?", false)? {
        let soul = Soul::default();
        soul.save()?;
        println!("   {} Soul reset to default!", style("✓").green());
    } else {
        println!("   {} Cancelled.", style("ℹ").blue());
    }

    Ok(())
}

/// Learn a new preference
fn learn_preference(text: &str) -> Result<()> {
    use openagent::agent::prompts::Soul;

    if text.is_empty() {
        println!("   {} No preference provided.", style("⚠").yellow());
        return Ok(());
    }

    let mut soul = Soul::load_or_default();
    soul.add_preference(text)?;
    
    println!();
    println!("   {} Learned: \"{}\"", style("🧠").bold(), style(text).cyan());
    println!("   {} Saved to SOUL.md", style("✓").green());
    println!();

    Ok(())
}

# OpenAgent

**OpenAgent** is a high-performance, low-latency, and secure AI agent framework built with **Rust**. It is a reimagined, compiled alternative to OpenClaw, offering enterprise-grade memory via a hybrid **PostgreSQL + OpenSearch** architecture and model flexibility through **OpenRouter**.

---

## ⚡ Key Features

* **Ultra-Low Latency:** Engineered in Rust for near-zero runtime overhead and high-concurrency handling.
* **Interactive CLI:** Beautiful terminal UI with arrow-key navigation, fuzzy search, and interactive menus powered by `dialoguer`.
* **Dynamic Port Selection:** Defaults to a random port in the **20000–29999** range for security and collision avoidance.
* **OpenRouter Integration:** Unified access to any LLM (DeepSeek, Claude, GPT-4, Llama) via a single API key.
* **Agent Soul (SOUL.md):** Customizable personality, values, and behavioral guidelines that evolve with conversations.
* **Docker Auto-Setup:** One-command database provisioning with automatic PostgreSQL and OpenSearch container management.
* **Hybrid Memory Engine:**
  * **PostgreSQL + pgvector:** For long-term semantic "memory" and structured metadata.
  * **OpenSearch:** For lightning-fast full-text search across massive conversation histories.
* **Telegram Native:** First-class support for Telegram Bot API as the primary command center.
* **Multi-Tier Sandboxing:** Securely run generated code in **OS**, **Sandbox (Wasm)**, or **Container** environments based on your security needs.

---

## 🚀 Quick Start

### Option A: Docker (Recommended)

The fastest way to get started - no Rust toolchain or pnpm required:

```bash
git clone https://github.com/OpenTech-Lab/openagent.git
cd openagent

# One-command setup (builds, starts databases, runs onboarding)
./docker-setup.sh
```

#### Multi-Agent Support

Create multiple isolated agents, each with their own databases, memory, and personality:

```bash
# Create agent "alice"
./docker-setup.sh alice

# Create another agent "bob"  
./docker-setup.sh bob

# Each agent has isolated:
# • PostgreSQL database (separate memory/records)
# • OpenSearch index (separate search history)
# • Workspace directory (.agents/<name>/workspace)
# • SOUL.md personality file (.agents/<name>/SOUL.md)
# • Environment config (.agents/<name>/.env)

# List all agents
./docker-setup.sh --list

# onboard agent
./docker-setup.sh alice --cli onboard

# Start specific agent
./docker-setup.sh alice --start

# Chat with specific agent
./docker-setup.sh alice --cli chat

# Stop specific agent
./docker-setup.sh alice --stop

# Remove agent and all its data
./docker-setup.sh alice --clean
```

**Manual Docker commands:**
```bash
# Build images
docker compose build

# Re-build images
docker compose build --no-cache

# Run the onboarding wizard
docker compose run --rm openagent-cli onboard

# Start the gateway
docker compose up -d openagent-gateway

# Run interactive chat
docker compose run --rm openagent-cli chat

# Check status
docker compose run --rm openagent-cli status

# View logs
docker compose logs -f

# Or
docker logs --tail 30 openagent-alice-gateway
```

**Docker Setup Script Options:**
```bash
./docker-setup.sh [agent-name]              # Full setup with onboarding
./docker-setup.sh [agent-name] --build      # Build images only
./docker-setup.sh [agent-name] --start      # Start all services
./docker-setup.sh [agent-name] --stop       # Stop all services
./docker-setup.sh [agent-name] --clean      # Remove all containers and data
./docker-setup.sh [agent-name] --cli chat   # Run CLI commands
./docker-setup.sh [agent-name] --status     # Show service status
./docker-setup.sh --list                    # List all agents
```

---

### Option B: Local Development (pnpm + Rust)

For development or if you prefer a local setup:

```bash
git clone https://github.com/OpenTech-Lab/openagent.git
cd openagent

# Install pnpm (optional)
# https://pnpm.io/installation

# Install dependencies (Rust toolchain & pnpm packages)
pnpm install

# Compile the Rust binaries
pnpm build
```

#### Interactive Setup Wizard

Run the interactive onboarding wizard with arrow-key navigation:

```bash
pnpm openagent onboard
```

The wizard will:
- ✅ Auto-detect available ports
- ✅ Guide you through API key configuration
- ✅ Offer to **auto-start PostgreSQL & OpenSearch via Docker**
- ✅ Let you browse and select AI models interactively
- ✅ Configure sandbox execution environment
- ✅ Run database migrations automatically

#### Start the Gateway

```bash
pnpm dev
```

#### (Optional) Interactive Main Menu

Run OpenAgent without arguments for a beautiful interactive menu:

```bash
pnpm openagent
```

---

## 🧠 Agent Soul

OpenAgent uses a `SOUL.md` file to define the agent's personality and behavior. This file is loaded as part of the system prompt and can be:

- **Viewed/Edited** via CLI: `pnpm openagent soul edit`
- **Updated during chat**: Use `/soul` command in interactive chat
- **Learned from conversations**: The agent can remember preferences

```bash
# View the soul
pnpm openagent soul view

# Edit in your default editor
pnpm openagent soul edit

# Add a learned preference
pnpm openagent soul learn "User prefers TypeScript over JavaScript"
```

---

## 💬 Interactive Chat

Start an interactive chat session with model selection:

```bash
pnpm openagent chat
```

**Chat Commands:**
| Command | Description |
|---------|-------------|
| `/quit` | Exit chat |
| `/clear` | Clear conversation history |
| `/model` | Browse and switch AI models |
| `/soul` | View/edit agent personality |
| `/help` | Show available commands |

---

## 🐳 Docker Database Setup

During onboarding, OpenAgent can automatically start databases via Docker:

```
📍 Step 4/5: Database Configuration (Optional)

   🐳 Docker detected - can auto-start databases

Select PostgreSQL setup:
> 🐳 Auto-start PostgreSQL with Docker (recommended)
  ⚙️  Configure existing PostgreSQL manually
  ⏭️  Skip PostgreSQL for now
```

Containers created:
- `openagent-postgres` - PostgreSQL 16 with pgvector
- `openagent-opensearch` - OpenSearch 2.x

---

## 🛠 Tech Stack

| Component | Technology | Role |
| --- | --- | --- |
| **Backend** | Rust (`tokio`) | Core logic, async task orchestration. |
| **Brain** | **OpenRouter** | Multi-model LLM gateway. |
| **Interface** | Telegram (`teloxide`) | User interaction and file handling. |
| **CLI** | `dialoguer` + `console` | Interactive terminal UI with arrow navigation. |
| **Vector DB** | PostgreSQL + `pgvector` | Semantic context and long-term memory. |
| **Search Engine** | OpenSearch | Keyword retrieval and historical message indexing. |
| **Orchestrator** | `pnpm` | Unified task management. |

---

## ⚙️ CLI Commands

```bash
# Interactive main menu
pnpm openagent

# Setup wizard
pnpm openagent onboard

# Initialize .env file
pnpm openagent init

# Interactive chat
pnpm openagent chat

# Browse AI models (fuzzy search)
pnpm openagent models

# View/edit agent soul
pnpm openagent soul [view|edit|reset|learn]

# Check service status
pnpm openagent status

# Test LLM connection
pnpm openagent test-llm

# Run database migrations
pnpm openagent migrate

# Execute code in sandbox
pnpm openagent run python "print('hello')"
```

---

## ⚙️ Environment Configuration

Create a `.env` file in the root directory. OpenAgent is designed to work with **OpenRouter** out of the box.

```env
# AI Configuration (OpenRouter)
OPENROUTER_API_KEY=your_openrouter_key_here
DEFAULT_MODEL=anthropic/claude-3.5-sonnet

# Messaging
TELEGRAM_BOT_TOKEN=your_telegram_bot_token

# Databases (auto-configured if using Docker setup)
DATABASE_URL=postgres://postgres:postgres@localhost:5432/openagent
OPENSEARCH_URL=http://localhost:9200

# Execution Security
# Options: 'os' (local dir), 'sandbox' (Wasm), 'container' (Docker)
EXECUTION_ENV=os
ALLOWED_DIR=/tmp/openagent-workspace
```

---

## 🛡 Security & Execution Environments

OpenAgent prioritizes the safety of your host machine. When the agent needs to run code or handle files, it uses the following hierarchy:

1. **OS Mode:** Runs commands within a restricted path (the installation/workspace directory) using non-privileged user permissions.
2. **Sandbox Mode (Recommended):** Uses **Wasmtime** to execute code in a high-speed, zero-access WebAssembly virtual machine.
3. **Container Mode:** Spins up an ephemeral, network-isolated Docker container for complex environment-dependent tasks.

---

## 🔧 Built-in Agent Tools

OpenAgent comes with several built-in tools that the AI agent can use to interact with the system:

| Tool | Description |
|------|-------------|
| `read_file` | Read file contents from the workspace directory |
| `write_file` | Write/create files in the workspace directory |
| `system_command` | Execute OS commands (apt, mv, ls, cat, etc.) |
| `duckduckgo_search` | Web search via DuckDuckGo (no API key required) |
| `brave_search` | Web search via Brave API (requires `BRAVE_API_KEY`) |
| `perplexity_search` | AI-powered search via Perplexity (requires `PERPLEXITY_API_KEY`) |

### System Command Tool

The `system_command` tool allows the agent to execute shell commands on the host OS:

```json
{
  "command": "apt",
  "args": ["update"]
}
```

**Examples:**
- `apt update` / `apt install -y nginx` (install packages)
- `service nginx start` (start services)
- `curl`, `wget` (download files)
- `mv old.txt new.txt`, `cp`, `mkdir -p`
- `ls -la /path/to/dir`, `cat file.txt`

**Package Installation (Docker):**

When running in Docker, the agent can install and configure software:

```
User: "Set up a basic nginx web server"

Agent will:
1. apt update
2. apt install -y nginx
3. Write config to /etc/nginx/sites-available/
4. service nginx start
```

This works because the Docker container runs as root, so `sudo` is not needed.

**Security:**
- Commands run with a configurable timeout (default: 60 seconds)
- Working directory is restricted to the configured `ALLOWED_DIR`
- Returns stdout, stderr, and exit code for transparency
- **Default denylist:** `rm`, `sudo`, `su`, `chmod`, `chown`, `mkfs`, `dd`, `shutdown`, `reboot`, `kill`, etc.
- **Shell injection protection:** Arguments are validated for dangerous characters (`;`, `|`, `` ` ``, `$(`, etc.)

---

## 🔐 OpenClaw-Style Security Features

OpenAgent implements security patterns inspired by [OpenClaw](https://github.com/openclaw/openclaw):

### Session-Based Sandboxing

Different chat contexts have different permission levels:

| Session Type | Tools Available | System Commands |
|--------------|-----------------|-----------------|
| **DM (Private Chat)** | Full access | All commands (except denylist) |
| **Group Chat** | Sandboxed | Read-only: `ls`, `cat`, `head`, `tail`, `grep`, `find`, `echo`, `pwd`, `date` |

This ensures that group chats (potentially untrusted) have restricted access, while private DMs with approved users get full functionality.

### DM Pairing (User Approval)

New users must be approved before they can interact with the bot via DM:

1. **New user sends message** → Receives pairing code and user ID
2. **Admin runs** `/pending` → Sees list of pending requests
3. **Admin runs** `/approve <user_id>` → User is approved
4. **User can now interact** with the bot

**Admin Commands:**
| Command | Description |
|---------|-------------|
| `/approve <user_id>` | Approve a user for DM access |
| `/pending` | List pending pairing requests |

**Notes:**
- Users in `TELEGRAM_ALLOW_FROM` are automatically approved (admins)
- Group chats don't require pairing (but have sandboxed tools)
- Pairing state is stored in memory (resets on restart)

### Tool Permissions

The `SystemCommandTool` supports fine-grained control:

```rust
// Allow only specific commands
SystemCommandTool::new()
    .with_allowed_commands(vec!["ls", "cat", "echo"])

// Block specific commands
SystemCommandTool::new()
    .with_denied_commands(vec!["rm", "sudo", "su"])
```

---

## 📂 Project Structure

```text
.
├── docker-setup.sh       # 🐳 Quick start script for Docker
├── docker-compose.yml    # 🐳 Docker services configuration
├── Dockerfile            # 🐳 Multi-stage build for production
├── src/
│   ├── bin/              # Binary entry points: gateway & cli
│   ├── core/             # ✨ Core trait abstractions (NEW)
│   │   ├── mod.rs        #    LlmProvider, Channel, StorageBackend, CodeExecutor
│   │   └── traits.rs     #    Modular interfaces for loose coupling
│   ├── agent/            # LLM logic, conversation, tools
│   ├── config/           # ✨ Modular configuration (NEW)
│   │   ├── types/        #    Provider, Channel, Storage, Sandbox configs
│   │   ├── validation.rs #    Configuration validation
│   │   └── paths.rs      #    Standard directory paths
│   ├── database/         # PostgreSQL, OpenSearch, SQLite backends
│   ├── sandbox/          # Multi-tier execution (OS/Wasm/Container)
│   ├── plugin_sdk/       # ✨ Plugin SDK for extensions (NEW)
│   │   ├── traits.rs     #    Plugin trait definition
│   │   ├── manifest.rs   #    Plugin metadata
│   │   └── registry.rs   #    Dynamic plugin loading
│   └── gateway/          # ✨ WebSocket protocol (NEW)
│       └── protocol/     #    JSON-RPC style messaging
├── docs/                 # Design documentation
├── SOUL.md               # Agent personality configuration
├── Cargo.toml            # Rust dependencies
└── package.json          # pnpm scripts
```

---

## 🏗 Architecture

OpenAgent follows a **modular, loosely-coupled architecture** with clear separation of concerns:

```mermaid
graph TB
    subgraph "Channels"
        TG[Telegram]
        CLI[CLI]
        WS[WebSocket]
    end

    subgraph "Core"
        AGENT[Agent Client]
        PROV[LLM Providers]
        TOOLS[Tool Manager]
    end

    subgraph "Storage"
        PG[(PostgreSQL)]
        OS[(OpenSearch)]
    end

    subgraph "Execution"
        SANDBOX[Sandbox Manager]
    end

    TG --> AGENT
    CLI --> AGENT
    WS --> AGENT
    
    AGENT --> PROV
    AGENT --> TOOLS
    AGENT --> PG
    AGENT --> OS
    
    TOOLS --> SANDBOX
```

### Core Traits

| Trait | Purpose |
|-------|---------|
| `LlmProvider` | Abstract LLM interface (OpenRouter, Anthropic, OpenAI) |
| `Channel` | Messaging platform interface (Telegram, Discord, etc.) |
| `StorageBackend` | Persistence layer (PostgreSQL, OpenSearch, SQLite) |
| `CodeExecutor` | Code execution sandbox (OS, Wasm, Container) |
| `Plugin` | Extension interface for custom functionality |

---

## 📚 Documentation

| Document | Description |
|----------|-------------|
| [Documentation Index](docs/README.md) | Overview and quick links |
| [Architecture](docs/architecture.md) | System design and module structure |
| [Core Traits](docs/core-traits.md) | LlmProvider, Channel, Storage, Executor |
| [Configuration](docs/configuration.md) | Config file format and options |
| [Agent Module](docs/agent.md) | Conversation and tool management |
| [Database Module](docs/database.md) | PostgreSQL, OpenSearch, vectors |
| [Sandbox Module](docs/sandbox.md) | Code execution environments |
| [Channels](docs/channels.md) | Telegram, Discord, Slack |
| [Gateway Protocol](docs/gateway-protocol.md) | WebSocket JSON-RPC protocol |
| [Plugin SDK](docs/plugin-sdk.md) | Building custom plugins |
| [Legacy Design](docs/DESIGN.md) | Original comprehensive design |
| [SOUL.md](SOUL.md) | Agent personality configuration |

---

## 📄 License

MIT License - see [LICENSE](LICENSE) for details.

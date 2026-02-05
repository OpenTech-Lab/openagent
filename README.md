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

### 1. Clone & Install

```bash
git clone https://github.com/OpenTech-Lab/openagent.git
cd openagent

# Install pnpm(option)
https://pnpm.io/installation

# Install dependencies (Rust toolchain & pnpm packages)
pnpm install

# Compile the Rust binaries
pnpm build
```

### 2. Interactive Setup Wizard

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

### 3. Start the Gateway

```bash
pnpm dev
```

### 4. (Optional) Interactive Main Menu

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

## 📂 Project Structure

```text
.
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

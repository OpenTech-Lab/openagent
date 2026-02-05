# OpenAgent

**OpenAgent** is a high-performance, low-latency, and secure AI agent framework built with **Rust**. It is a reimagined, compiled alternative to OpenClaw, offering enterprise-grade memory via a hybrid **PostgreSQL + OpenSearch** architecture and model flexibility through **OpenRouter**.

---

## ⚡ Key Features

* **Ultra-Low Latency:** Engineered in Rust for near-zero runtime overhead and high-concurrency handling.
* **OpenRouter Integration:** Unified access to any LLM (DeepSeek, Claude, GPT-4, Llama) via a single API key.
* **Hybrid Memory Engine:**
* **PostgreSQL + pgvector:** For long-term semantic "memory" and structured metadata.
* **OpenSearch:** For lightning-fast full-text search across massive conversation histories.


* **Telegram Native:** First-class support for Telegram Bot API as the primary command center.
* **Multi-Tier Sandboxing:** Securely run generated code in **OS**, **Sandbox (Wasm)**, or **Container** environments based on your security needs.

---

## 🚀 Quick Start

### 1. Clone & Install

```bash
git clone https://github.com/your-org/openagent.git
cd openagent

# Install dependencies (Rust toolchain & UI packages)
pnpm install

# Build the UI components
pnpm ui:build

# Compile the Rust binaries
pnpm build

```

### 2. Onboarding & Configuration

Initialize your environment, verify database connections, and set up your daemon.

```bash
pnpm openagent onboard --install-daemon

```

### 3. Development Mode

Run the gateway with hot-reloading for rapid development.

```bash
pnpm gateway:watch

```

---

## 🛠 Tech Stack

| Component | Technology | Role |
| --- | --- | --- |
| **Backend** | Rust (`tokio`) | Core logic, async task orchestration. |
| **Brain** | **OpenRouter** | Multi-model LLM gateway. |
| **Interface** | Telegram (`teloxide`) | User interaction and file handling. |
| **Vector DB** | PostgreSQL + `pgvector` | Semantic context and long-term memory. |
| **Search Engine** | OpenSearch | Keyword retrieval and historical message indexing. |
| **Orchestrator** | `pnpm` | Unified task management. |

---

## ⚙️ Environment Configuration

Create a `.env` file in the root directory. OpenAgent is designed to work with **OpenRouter** out of the box.

```env
# AI Configuration (OpenRouter)
OPENROUTER_API_KEY=your_openrouter_key_here
DEFAULT_MODEL=anthropic/claude-3.5-sonnet # Example OpenRouter model ID

# Messaging
TELEGRAM_BOT_TOKEN=your_telegram_bot_token

# Databases
DATABASE_URL=postgres://user:pass@localhost:5432/openagent
OPENSEARCH_URL=https://localhost:9200

# Execution Security
# Options: 'os' (local dir), 'sandbox' (Wasm), 'container' (Docker)
EXECUTION_ENV=sandbox
ALLOWED_DIR=./workspace

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
│   ├── bin/           # Binary entry points: gateway (Telegram) & cli (onboard)
│   ├── agent/         # LLM logic, Prompt Engineering, & OpenRouter client
│   ├── database/      # PostgreSQL and OpenSearch logic
│   └── sandbox/       # Security abstraction layers (OS/Wasm/Docker)
├── ui/                # Dashboard / Management interface
├── Cargo.toml         # Rust dependencies (teloxide, sqlx, opensearch, etc.)
└── package.json       # PNPM scripts for developer workflow

```

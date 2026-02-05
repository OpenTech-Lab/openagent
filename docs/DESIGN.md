# OpenAgent System Design

> **Note:** This document is part of the modular documentation. For a quick overview, see the [Documentation Index](./README.md). For detailed module documentation, see:
> - [Architecture](./architecture.md) - System design principles
> - [Core Traits](./core-traits.md) - Trait abstractions
> - [Configuration](./configuration.md) - Config system
> - [Agent](./agent.md) | [Database](./database.md) | [Sandbox](./sandbox.md) | [Channels](./channels.md)
> - [Gateway Protocol](./gateway-protocol.md) | [Plugin SDK](./plugin-sdk.md)

This document describes the architecture, data flows, and component interactions of OpenAgent.

## Table of Contents

- [System Overview](#system-overview)
- [Architecture](#architecture)
- [Component Details](#component-details)
- [Data Flows](#data-flows)
- [CLI & User Experience](#cli--user-experience)
- [Agent Soul](#agent-soul)
- [Security Model](#security-model)
- [Deployment](#deployment)

---

## System Overview

OpenAgent is a high-performance AI agent framework built in Rust that provides:

- **Multi-model LLM access** via OpenRouter
- **Hybrid memory** using PostgreSQL + pgvector and OpenSearch
- **Secure code execution** through multi-tier sandboxing
- **Telegram bot interface** as the primary user interaction layer
- **Interactive CLI** with arrow-key navigation and fuzzy search
- **Customizable Agent Soul** for personality and behavior configuration

```mermaid
graph TB
    subgraph "User Interface"
        TG[Telegram Bot]
        CLI[CLI Tool]
    end

    subgraph "Core Engine"
        GW[Gateway Service]
        AG[Agent Logic]
        CM[Conversation Manager]
    end

    subgraph "External Services"
        OR[OpenRouter API]
        LLM1[Claude]
        LLM2[GPT-4]
        LLM3[DeepSeek]
    end

    subgraph "Storage Layer"
        PG[(PostgreSQL + pgvector)]
        OS[(OpenSearch)]
    end

    subgraph "Execution Layer"
        SB[Sandbox Manager]
        OSE[OS Executor]
        WE[Wasm Executor]
        CE[Container Executor]
    end

    TG --> GW
    CLI --> AG
    GW --> AG
    AG --> CM
    AG --> OR
    OR --> LLM1
    OR --> LLM2
    OR --> LLM3
    CM --> PG
    AG --> OS
    AG --> SB
    SB --> OSE
    SB --> WE
    SB --> CE
```

---

## Architecture

### High-Level Architecture

```mermaid
C4Context
    title OpenAgent System Context

    Person(user, "User", "Interacts via Telegram or CLI")

    System(openagent, "OpenAgent", "AI Agent Framework")

    System_Ext(openrouter, "OpenRouter", "LLM Gateway API")
    System_Ext(telegram, "Telegram", "Messaging Platform")

    SystemDb_Ext(postgres, "PostgreSQL", "Vector DB + Metadata")
    SystemDb_Ext(opensearch, "OpenSearch", "Full-text Search")
    System_Ext(docker, "Docker", "Container Runtime")

    Rel(user, telegram, "Sends messages")
    Rel(telegram, openagent, "Webhook/Polling")
    Rel(openagent, openrouter, "LLM Requests")
    Rel(openagent, postgres, "Store/Query")
    Rel(openagent, opensearch, "Search")
    Rel(openagent, docker, "Execute code")
```

### Module Structure

OpenAgent follows a modular, loosely-coupled architecture inspired by [openclaw](https://github.com/openclaw/openclaw):

```mermaid
graph LR
    subgraph "openagent (lib)"
        LIB[lib.rs]

        subgraph "core/"
            PROV_TRAIT[provider.rs<br/>LlmProvider trait]
            CHAN_TRAIT[channel.rs<br/>Channel trait]
            STOR_TRAIT[storage.rs<br/>StorageBackend trait]
            EXEC_TRAIT[executor.rs<br/>CodeExecutor trait]
            CORE_TYPES[types.rs<br/>Message, Role]
        end

        subgraph "config/"
            CFG_MOD[mod.rs]
            CFG_IO[io.rs<br/>load/save]
            CFG_PATHS[paths.rs<br/>directories]
            CFG_VALID[validation.rs]
            subgraph "types/"
                CFG_PROV[provider.rs]
                CFG_CHAN[channel.rs]
                CFG_STOR[storage.rs]
                CFG_SAND[sandbox.rs]
            end
        end

        subgraph "agent/"
            CLIENT[client.rs]
            CONV[conversation.rs]
            PROMPT[prompts.rs]
            TOOLS[tools.rs]
            TYPES[types.rs]
        end

        subgraph "database/"
            PG[postgres.rs]
            OSEARCH[opensearch.rs]
            MEM[memory.rs]
        end

        subgraph "sandbox/"
            EXEC[executor.rs]
            OSSB[os_sandbox.rs]
            WASM[wasm.rs]
            CONT[container.rs]
        end

        subgraph "gateway/"
            GW_MOD[mod.rs]
            subgraph "protocol/"
                SCHEMA[schema.rs<br/>GatewayFrame]
                GW_TYPES[types.rs<br/>Requests/Events]
            end
        end

        subgraph "plugin_sdk/"
            PLUGIN_MOD[mod.rs]
            MANIFEST[manifest.rs]
            TRAITS[traits.rs<br/>Plugin trait]
            REGISTRY[registry.rs<br/>PluginRegistry]
        end

        ERR[error.rs]
    end

    subgraph "binaries"
        GATEWAY[bin/gateway.rs]
        CLITOOL[bin/cli.rs]
    end

    LIB --> PROV_TRAIT
    LIB --> CHAN_TRAIT
    LIB --> STOR_TRAIT
    LIB --> EXEC_TRAIT
    LIB --> CFG_MOD
    LIB --> PLUGIN_MOD
    LIB --> GW_MOD

    GATEWAY --> LIB
    CLITOOL --> LIB
```

### Design Principles

OpenAgent's architecture follows these core principles:

1. **Trait-based Abstraction**: All major components (providers, channels, storage, executors) are defined as traits, enabling loose coupling and easy testing.

2. **Modular Configuration**: Configuration is split into focused modules rather than a single monolithic file:
   - `config/types/provider.rs` - LLM provider settings
   - `config/types/channel.rs` - Messaging channel settings
   - `config/types/storage.rs` - Database/storage settings
   - `config/types/sandbox.rs` - Code execution settings

3. **Plugin Architecture**: The `plugin_sdk` module provides interfaces for extending OpenAgent:
   - Register new LLM providers
   - Add messaging channels
   - Implement custom storage backends
   - Create new code executors

4. **Gateway Protocol**: WebSocket-based JSON protocol for client communication:
   - Request/Response pattern with message IDs
   - Event streaming for real-time updates
   - Session management
   - Authentication support

### Core Traits

```mermaid
classDiagram
    class LlmProvider {
        <<trait>>
        +name() str
        +generate(messages, options) Result~LlmResponse~
        +stream(messages, options) Stream~Chunk~
    }

    class Channel {
        <<trait>>
        +name() str
        +start() Result
        +stop() Result
        +send(reply) Result
        +capabilities() ChannelCapabilities
    }

    class StorageBackend {
        <<trait>>
        +name() str
        +save_conversation(conv) Result~String~
        +load_conversation(id) Result~Option~
        +delete_conversation(id) Result
        +list_conversations(user_id) Result~Vec~
    }

    class CodeExecutor {
        <<trait>>
        +name() str
        +supports_language(lang) bool
        +execute(request) Result~ExecutionResult~
    }

    class Plugin {
        <<trait>>
        +manifest() PluginManifest
        +on_load(api) Result
        +on_unload() Result
    }
```

---

## Component Details

### 1. Agent Module

The agent module handles all LLM-related functionality.

```mermaid
classDiagram
    class OpenRouterClient {
        -client: HttpClient
        -config: OpenRouterConfig
        -rate_limit: RateLimitState
        +new(config) Result~Self~
        +chat(messages, options) Result~Response~
        +chat_with_model(model, messages, options) Result~Response~
        +chat_with_tools(messages, tools, options) Result~Response~
        +list_models() Result~Vec~ModelInfo~~
    }

    class ConversationManager {
        -conversations: HashMap~String, Conversation~
        -default_model: String
        -default_system_prompt: Option~String~
        +new(model) Self
        +get_or_create(user_id) Conversation
        +get(user_id) Option~Conversation~
        +clear_conversation(user_id)
    }

    class Conversation {
        +id: UUID
        +user_id: String
        +messages: Vec~Message~
        +system_prompt: Option~String~
        +model: String
        +total_tokens: u32
        +add_user_message(content)
        +add_assistant_message(content)
        +get_api_messages() Vec~Message~
    }

    class ToolRegistry {
        -tools: HashMap~String, Box~dyn Tool~~
        +register(tool)
        +get(name) Option~Tool~
        +execute(call) Result~ToolResult~
        +definitions() Vec~ToolDefinition~
    }

    ConversationManager --> Conversation
    OpenRouterClient ..> Conversation
    ToolRegistry --> Tool
```

### 2. Database Module

Hybrid storage combining vector search and full-text search.

```mermaid
classDiagram
    class PostgresPool {
        <<type alias>>
        PgPool
    }

    class MemoryStore {
        -pg_pool: PostgresPool
        -opensearch: Option~OpenSearchClient~
        +new(pool, opensearch) Self
        +save(memory, embedding) Result
        +get(id) Result~Option~Memory~~
        +search_semantic(user_id, embedding, limit) Result~Vec~Memory~~
        +search_fulltext(user_id, query, limit) Result~Vec~Memory~~
    }

    class Memory {
        +id: UUID
        +user_id: String
        +content: String
        +summary: Option~String~
        +importance: f32
        +tags: Vec~String~
        +created_at: DateTime
        +access_count: i32
    }

    class OpenSearchClient {
        -client: OpenSearch
        -index_prefix: String
        +new(config) Result~Self~
        +index_document(doc) Result
        +search(doc_type, query, user_id, limit) Result~Vec~SearchResult~~
        +health_check() Result
    }

    MemoryStore --> PostgresPool
    MemoryStore --> OpenSearchClient
    MemoryStore --> Memory
```

### 3. Sandbox Module

Multi-tier code execution with security isolation.

```mermaid
classDiagram
    class CodeExecutor {
        <<trait>>
        +name() str
        +supports_language(lang) bool
        +execute(request) Result~ExecutionResult~
    }

    class ExecutionRequest {
        +code: String
        +language: Language
        +stdin: Option~String~
        +timeout: Duration
        +env: HashMap~String, String~
    }

    class ExecutionResult {
        +success: bool
        +exit_code: Option~i32~
        +stdout: String
        +stderr: String
        +execution_time: Duration
        +timed_out: bool
    }

    class OsSandbox {
        -allowed_dir: PathBuf
        +new(dir) Self
    }

    class WasmExecutor {
        -engine: wasmtime::Engine
        +new() Result~Self~
    }

    class ContainerExecutor {
        -docker: Docker
        -config: ContainerConfig
        +new(config) Result~Self~
    }

    CodeExecutor <|.. OsSandbox
    CodeExecutor <|.. WasmExecutor
    CodeExecutor <|.. ContainerExecutor
    CodeExecutor --> ExecutionRequest
    CodeExecutor --> ExecutionResult
```

---

## Data Flows

### 1. Message Processing Flow

```mermaid
sequenceDiagram
    participant U as User
    participant T as Telegram
    participant G as Gateway
    participant C as ConversationManager
    participant O as OpenRouterClient
    participant L as LLM (via OpenRouter)
    participant D as Database

    U->>T: Send message
    T->>G: Webhook/Poll update
    G->>G: Validate user permissions
    G->>C: Get or create conversation
    C-->>G: Conversation context
    G->>G: Add user message to conversation
    G->>O: Send chat request
    O->>L: API call with messages
    L-->>O: Generated response
    O-->>G: ChatCompletionResponse
    G->>C: Add assistant message
    G->>D: Persist conversation (optional)
    G->>T: Send reply
    T->>U: Display message
```

### 2. Code Execution Flow

```mermaid
sequenceDiagram
    participant U as User
    participant G as Gateway
    participant S as SandboxManager
    participant E as Executor (OS/Wasm/Container)
    participant R as Runtime

    U->>G: /run python print("hello")
    G->>G: Parse language and code
    G->>S: Create ExecutionRequest

    alt OS Mode
        S->>E: OsSandbox.execute()
        E->>R: spawn process in allowed_dir
        R-->>E: Process output
    else Wasm Mode
        S->>E: WasmExecutor.execute()
        E->>R: Run in Wasmtime VM
        R-->>E: Execution result
    else Container Mode
        S->>E: ContainerExecutor.execute()
        E->>R: Create ephemeral Docker container
        R->>R: Execute code
        R-->>E: Container logs
        E->>R: Remove container
    end

    E-->>S: ExecutionResult
    S-->>G: Result with stdout/stderr
    G->>U: Display formatted output
```

### 3. Memory Search Flow

```mermaid
sequenceDiagram
    participant A as Agent
    participant M as MemoryStore
    participant P as PostgreSQL
    participant O as OpenSearch
    participant E as Embedding Service

    A->>M: search_semantic(query)
    A->>E: Generate query embedding
    E-->>A: Vector [f32; 1536]
    A->>M: search_semantic(user_id, embedding)
    M->>P: SELECT ... ORDER BY embedding <=> $1
    P-->>M: Similar memories by vector distance
    M-->>A: Vec<(Memory, similarity)>

    A->>M: search_fulltext(query)
    M->>O: Multi-match query
    O-->>M: SearchResults with highlights
    M->>P: Fetch full Memory records
    P-->>M: Memory details
    M-->>A: Vec<Memory>
```

### 4. Tool Execution Flow

```mermaid
sequenceDiagram
    participant U as User
    participant G as Gateway
    participant O as OpenRouterClient
    participant L as LLM
    participant T as ToolRegistry
    participant Tool as Tool Implementation

    U->>G: Complex request requiring tools
    G->>O: chat_with_tools(messages, tools)
    O->>L: Request with tool definitions
    L-->>O: Response with tool_calls

    loop For each tool call
        O->>G: Return tool call request
        G->>T: execute(tool_call)
        T->>Tool: tool.execute(args)
        Tool-->>T: ToolResult
        T-->>G: Result
        G->>O: Continue with tool results
    end

    O->>L: Final request with tool results
    L-->>O: Final response
    O-->>G: ChatCompletionResponse
    G->>U: Display response
```

---

## CLI & User Experience

### Interactive Terminal Interface

OpenAgent features a modern interactive CLI built with `dialoguer` and `console` crates, providing:

- **Arrow-key navigation** for menu selection
- **Fuzzy search** for model browsing
- **Colored output** with progress indicators
- **Interactive prompts** with defaults and validation

```mermaid
flowchart TD
    A[pnpm openagent] --> B{Command provided?}
    B -->|No| C[Interactive Main Menu]
    B -->|Yes| D[Execute Command]
    
    C --> E{User Selection}
    E -->|Chat| F[Interactive Chat]
    E -->|Soul| G[Soul Management]
    E -->|Onboard| H[Setup Wizard]
    E -->|Models| I[Model Browser]
    E -->|Status| J[Service Check]
    
    F --> K[Model Selection]
    K --> L[Chat Loop]
    L --> M{/command?}
    M -->|/quit| N[Exit]
    M -->|/model| K
    M -->|/soul| G
    M -->|/clear| O[Clear History]
    M -->|No| P[Send to LLM]
    P --> L
```

### CLI Command Structure

```mermaid
graph LR
    subgraph "openagent CLI"
        ROOT[openagent]
        
        ROOT --> INIT[init]
        ROOT --> ONBOARD[onboard]
        ROOT --> CHAT[chat]
        ROOT --> MODELS[models]
        ROOT --> SOUL[soul]
        ROOT --> STATUS[status]
        ROOT --> MIGRATE[migrate]
        ROOT --> RUN[run]
        ROOT --> TEST[test-llm]
        
        SOUL --> SV[view]
        SOUL --> SE[edit]
        SOUL --> SR[reset]
        SOUL --> SL[learn]
    end
```

### Docker Auto-Setup Flow

```mermaid
sequenceDiagram
    participant U as User
    participant C as CLI
    participant D as Docker
    participant PG as PostgreSQL
    participant OS as OpenSearch
    
    U->>C: pnpm openagent onboard
    C->>C: Check Docker availability
    C->>U: Show database options menu
    U->>C: Select "Auto-start with Docker"
    
    C->>D: docker run openagent-postgres
    D->>PG: Start PostgreSQL container
    C->>C: Wait for pg_isready
    PG-->>C: Ready
    
    C->>D: docker run openagent-opensearch
    D->>OS: Start OpenSearch container
    C->>C: Wait for HTTP 200
    OS-->>C: Ready
    
    C->>U: Offer to run migrations
    U->>C: Yes
    C->>PG: Run migrations
    C->>U: Setup complete!
```

---

## Agent Soul

### Overview

The Agent Soul (`SOUL.md`) defines the agent's personality, values, and behavioral guidelines. It's loaded as part of the system prompt and can evolve through conversations.

```mermaid
classDiagram
    class Soul {
        +content: String
        +path: String
        +load() Result~Soul~
        +load_or_default() Soul
        +save() Result
        +as_system_prompt() String
        +update_section(section, content) Result
        +add_preference(pref) Result
        +add_topic(topic) Result
        +add_context(ctx) Result
    }
    
    class Conversation {
        +system_prompt: Option~String~
        +with_system_prompt(prompt) Self
    }
    
    Soul --> Conversation : provides system prompt
```

### Soul Structure

```mermaid
mindmap
    root((SOUL.md))
        Identity
            Name
            Role
        Personality
            Helpful
            Curious
            Honest
            Concise
            Friendly
        Core Values
            Accuracy
            Privacy
            Transparency
            Safety
            Learning
        Communication
            Language style
            Formatting
            Emoji usage
        Expertise
            Programming
            Code review
            System design
        Boundaries
            Limitations
            Ethics
        Memory
            User Preferences
            Frequent Topics
            Important Context
```

### Soul Update Flow

```mermaid
sequenceDiagram
    participant U as User
    participant C as Chat/CLI
    participant S as Soul
    participant F as Filesystem
    
    alt View Soul
        U->>C: /soul or soul view
        C->>S: Soul::load()
        S->>F: Read SOUL.md
        F-->>S: Content
        S-->>C: Soul instance
        C->>U: Display formatted soul
    end
    
    alt Edit Soul
        U->>C: soul edit
        C->>C: Detect $EDITOR
        C->>F: Open SOUL.md in editor
        U->>F: Edit and save
        C->>U: Soul updated!
    end
    
    alt Learn Preference
        U->>C: soul learn "preference"
        C->>S: Soul::load()
        C->>S: add_preference(pref)
        S->>S: Update section
        S->>S: Update timestamp
        S->>F: Save SOUL.md
        C->>U: Learned!
    end
```

---

## Security Model

### Execution Environment Hierarchy

```mermaid
graph TB
    subgraph "Security Levels"
        direction TB
        L1[Level 1: Container Mode]
        L2[Level 2: Wasm Mode]
        L3[Level 3: OS Mode]
    end

    subgraph "Container Mode - Highest Security"
        C1[Network Isolated]
        C2[Memory Limited]
        C3[CPU Limited]
        C4[Ephemeral Filesystem]
        C5[No Host Access]
    end

    subgraph "Wasm Mode - High Security"
        W1[Zero Host Access]
        W2[Fuel-based Limits]
        W3[Memory Sandboxed]
        W4[No I/O by Default]
    end

    subgraph "OS Mode - Basic Security"
        O1[Restricted Directory]
        O2[Process Timeout]
        O3[Unprivileged User]
    end

    L1 --> C1
    L1 --> C2
    L1 --> C3
    L1 --> C4
    L1 --> C5

    L2 --> W1
    L2 --> W2
    L2 --> W3
    L2 --> W4

    L3 --> O1
    L3 --> O2
    L3 --> O3

    style L1 fill:#4CAF50
    style L2 fill:#8BC34A
    style L3 fill:#FFC107
```

### Authentication & Authorization

```mermaid
flowchart TD
    A[Incoming Request] --> B{Source?}

    B -->|Telegram| C[Extract User ID]
    B -->|CLI| D[Local User]

    C --> E{User in allowed_users?}
    E -->|Yes| F[Authorize]
    E -->|No & List Empty| F
    E -->|No & List Set| G[Reject]

    D --> F

    F --> H[Process Request]
    G --> I[Return Unauthorized]

    subgraph "Secrets Management"
        S1[API Keys in .env]
        S2[SecretString wrapper]
        S3[No logging of secrets]
    end
```

---

## Deployment

### Docker Compose Architecture

```mermaid
graph TB
    subgraph "Docker Network"
        subgraph "Application"
            OA[openagent-gateway]
        end

        subgraph "Databases"
            PG[(postgres:15-alpine)]
            OS[(opensearchproject/opensearch)]
        end

        subgraph "Execution Sandbox"
            DK[Docker-in-Docker / Socket]
        end
    end

    subgraph "External"
        TG[Telegram API]
        OR[OpenRouter API]
    end

    OA --> PG
    OA --> OS
    OA --> DK
    OA <--> TG
    OA --> OR
```

### System Requirements

```mermaid
mindmap
    root((OpenAgent))
        Runtime
            Rust 1.70+
            Docker (optional)
        Databases
            PostgreSQL 15+
                pgvector extension
            OpenSearch 2.x
                (optional)
        External APIs
            OpenRouter API Key
            Telegram Bot Token
        Resources
            2GB+ RAM
            10GB+ Storage
            Network Access
```

### Configuration Flow

```mermaid
flowchart LR
    subgraph "Configuration Sources"
        ENV[.env file]
        ENVVAR[Environment Variables]
    end

    subgraph "Config Loading"
        LOAD[dotenvy::dotenv]
        PARSE[Config::from_env]
    end

    subgraph "Config Sections"
        OR_CFG[OpenRouterConfig]
        TG_CFG[TelegramConfig]
        DB_CFG[DatabaseConfig]
        OS_CFG[OpenSearchConfig]
        SB_CFG[SandboxConfig]
    end

    ENV --> LOAD
    ENVVAR --> LOAD
    LOAD --> PARSE
    PARSE --> OR_CFG
    PARSE --> TG_CFG
    PARSE --> DB_CFG
    PARSE --> OS_CFG
    PARSE --> SB_CFG
```

### Onboarding Flow

The CLI provides an interactive onboarding experience with arrow-key navigation and Docker auto-setup.

```mermaid
flowchart TD
    A[openagent onboard] --> B{.env exists?}
    B -->|No| C[Create from template]
    B -->|Yes| D[Load existing config]
    
    C --> E[Step 1: Port Config]
    D --> E
    
    E --> F[Find free port 20000-29999]
    F --> G[Step 2: OpenRouter]
    G --> H[Enter API key]
    H --> I[Select default model]
    I --> J[Step 3: Telegram]
    J --> K[Enter bot token]
    K --> L[Step 4: Database]
    
    L --> M{Docker available?}
    M -->|Yes| N[Show Docker options menu]
    M -->|No| O[Show manual config]
    
    N --> P{User choice?}
    P -->|Auto-start| Q[Start PostgreSQL container]
    P -->|Manual| O
    P -->|Skip| R[Continue without DB]
    
    Q --> S[Wait for ready]
    S --> T[Start OpenSearch container]
    T --> U[Wait for ready]
    U --> V{Run migrations?}
    V -->|Yes| W[Execute migrations]
    V -->|No| X[Step 5: Sandbox]
    W --> X
    
    O --> X
    R --> X
    
    X --> Y[Select execution env]
    Y --> Z[Save .env]
    Z --> AA[Verify connections]
    AA --> AB[Show summary]
```

### Port Discovery Algorithm

```mermaid
flowchart TD
    A[Start: port = 20000] --> B{port <= 29999?}
    B -->|Yes| C[Try bind 127.0.0.1:port]
    B -->|No| D[Return None]

    C --> E{Bind successful?}
    E -->|Yes| F[Release socket]
    E -->|No| G[port++]

    F --> H[Return Some port]
    G --> B

    style H fill:#4CAF50
    style D fill:#f44336
```

### Docker Container Management

```mermaid
flowchart TD
    subgraph "PostgreSQL Setup"
        PA[Check if running] --> PB{Running?}
        PB -->|Yes| PC[Use existing]
        PB -->|No| PD{Container exists?}
        PD -->|Yes| PE[docker start]
        PD -->|No| PF[docker run]
        PE --> PG[Wait pg_isready]
        PF --> PG
        PG --> PH[Ready]
    end
    
    subgraph "OpenSearch Setup"
        OA[Check if running] --> OB{Running?}
        OB -->|Yes| OC[Use existing]
        OB -->|No| OD{Container exists?}
        OD -->|Yes| OE[docker start]
        OD -->|No| OF[docker run]
        OE --> OG[Wait HTTP 200]
        OF --> OG
        OG --> OH[Ready]
    end
```

---

## Error Handling

```mermaid
graph TD
    subgraph "Error Types"
        E1[Config Error]
        E2[OpenRouter Error]
        E3[Database Error]
        E4[OpenSearch Error]
        E5[Telegram Error]
        E6[Sandbox Error]
        E7[Wasm Error]
        E8[Container Error]
        E9[HTTP Error]
        E10[I/O Error]
    end

    subgraph "Error Properties"
        P1{is_retryable?}
        P2{is_client_error?}
    end

    E2 --> P1
    E3 --> P1
    E4 --> P1
    E9 --> P1

    E1 --> P2
    E5 --> P2

    P1 -->|Yes| R[Retry with backoff]
    P1 -->|No| F[Fail immediately]
    P2 -->|Yes| U[Return user-friendly message]
    P2 -->|No| L[Log and continue]
```

---

## Future Considerations

```mermaid
timeline
    title OpenAgent Roadmap

    section Phase 1 - Foundation
        Core Framework : Telegram Gateway
                       : OpenRouter Integration
                       : Basic Sandboxing

    section Phase 2 - Memory
        Hybrid Memory : PostgreSQL + pgvector
                     : OpenSearch Integration
                     : Conversation Persistence

    section Phase 3 - Tools
        Tool Framework : File Operations
                      : Code Execution
                      : Web Search

    section Phase 4 - Advanced
        Multi-Agent : Agent Collaboration
                   : Task Delegation
                   : Workflow Automation
```

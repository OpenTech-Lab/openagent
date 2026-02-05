# OpenAgent Documentation

Welcome to the OpenAgent documentation. This documentation covers the architecture, design, and usage of the OpenAgent AI framework.

## Documentation Index

### Architecture & Design

- [Architecture Overview](./architecture.md) - High-level system architecture and design principles
- [Core Traits](./core-traits.md) - Trait-based abstractions for loose coupling
- [Configuration](./configuration.md) - Modular configuration system
- [Gateway Protocol](./gateway-protocol.md) - WebSocket-based control plane
- [Plugin SDK](./plugin-sdk.md) - Building extensions and plugins

### Component Documentation

- [Agent Module](./agent.md) - LLM integration and conversation management
- [Database Module](./database.md) - Hybrid storage with PostgreSQL and OpenSearch
- [Sandbox Module](./sandbox.md) - Multi-tier code execution environments
- [Channels](./channels.md) - Messaging platform integrations

### Reference

- [API Reference](./api-reference.md) - Complete API documentation
- [System Design](./DESIGN.md) - Comprehensive system design document (legacy)

## Quick Links

- [Main README](../README.md) - Project overview and quick start
- [SOUL.md](../SOUL.md) - Agent personality configuration
- [LICENSE](../LICENSE) - MIT License

## Architecture at a Glance

```
┌─────────────────────────────────────────────────────────────────┐
│                        OpenAgent Framework                       │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────┐ │
│  │ Channels │  │ Providers│  │ Storage  │  │    Executors     │ │
│  │ (Trait)  │  │ (Trait)  │  │ (Trait)  │  │     (Trait)      │ │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────────┬─────────┘ │
│       │             │             │                  │           │
│  ┌────┴─────┐  ┌────┴─────┐  ┌────┴─────┐  ┌────────┴─────────┐ │
│  │ Telegram │  │OpenRouter│  │PostgreSQL│  │   OS Sandbox     │ │
│  │ Discord  │  │Anthropic │  │OpenSearch│  │   Wasm Runtime   │ │
│  │ Slack    │  │ OpenAI   │  │ SQLite   │  │   Container      │ │
│  └──────────┘  └──────────┘  └──────────┘  └──────────────────┘ │
├─────────────────────────────────────────────────────────────────┤
│                         Plugin SDK                               │
│              (Extend with custom implementations)                │
└─────────────────────────────────────────────────────────────────┘
```

## Design Philosophy

OpenAgent is built on these core principles:

1. **Trait-based Abstraction** - All major components implement traits for loose coupling
2. **Modular Configuration** - Split configuration into focused, domain-specific modules
3. **Plugin Architecture** - Easy extension through the Plugin SDK
4. **Security First** - Multi-tier sandboxing for safe code execution
5. **Performance** - Built in Rust for minimal overhead and maximum concurrency

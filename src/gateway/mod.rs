//! Gateway module - WebSocket-based control plane
//!
//! This module provides the gateway server and client for OpenAgent,
//! following openclaw's architecture of a central control plane.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │                    Gateway Server                    │
//! │              ws://127.0.0.1:18789                   │
//! └───────────────────────┬─────────────────────────────┘
//!                         │
//!           ┌─────────────┼─────────────┐
//!           │             │             │
//!           ▼             ▼             ▼
//!      ┌─────────┐   ┌─────────┐   ┌─────────┐
//!      │   CLI   │   │ WebChat │   │ Channel │
//!      │ Client  │   │   UI    │   │ Plugins │
//!      └─────────┘   └─────────┘   └─────────┘
//! ```

pub mod protocol;

pub use protocol::{
    GatewayFrame, ProtocolVersion, PROTOCOL_VERSION,
    schema::error_codes,
};

pub use protocol::types::{
    AuthRequest, AuthResponse, AuthMethod,
    SessionInfo, SessionsListRequest, SessionsListResponse,
    AgentSendRequest, AgentResponse, UsageStats,
    ChannelStatus, ChannelsListRequest, ChannelsListResponse,
    StreamChunkEvent, MessageEvent,
    events,
};

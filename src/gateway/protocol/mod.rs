//! Gateway Protocol - WebSocket-based control plane
//!
//! This module defines the protocol for communication between clients and the
//! gateway server, following openclaw's WebSocket protocol pattern.
//!
//! ## Protocol Overview
//!
//! - **JSON-based messages** over WebSocket
//! - **Request-response pattern** with unique message IDs
//! - **Event streaming** for real-time updates
//! - **Authentication** via tokens or sessions
//!
//! ## Message Types
//!
//! - `Request`: Client-initiated requests
//! - `Response`: Server responses to requests
//! - `Event`: Server-pushed events (streaming, status updates)
//! - `Error`: Error responses

pub mod schema;
pub mod types;

pub use schema::{GatewayFrame, ProtocolVersion, PROTOCOL_VERSION};
pub use types::*;

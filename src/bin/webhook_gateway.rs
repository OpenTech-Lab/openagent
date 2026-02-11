//! OpenAgent Webhook Gateway
//!
//! Webhook-based alternative to polling gateway.
//! Receives Telegram updates via webhook, processes tasks asynchronously,
//! and sends results back via configurable webhook callbacks.

use axum::{
    routing::get,
    Router,
};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Build Axum router
    let app = Router::new()
        .route("/health", get(health_check));

    // Start server
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    println!("Webhook server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}
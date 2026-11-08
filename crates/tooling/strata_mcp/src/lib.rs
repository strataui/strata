//! Professional Model Context Protocol (MCP) Server for Strata Development Sessions.
//!
//! Provides a bidirectional JSON-RPC 2.0 interface enabling external developer tools and language models
//! to connect to a live running Strata application, inspect session telemetry,
//! take pixel-perfect screenshots with authentic Yaru cursor overlays, perform keyboard and mouse
//! interactions, and safely batch sequential UI action pipelines.

pub mod client;
pub mod protocol;
pub mod server;

pub use client::{default_socket_path, StrataDevClient};
pub use protocol::*;
pub use server::McpServer;

/// Runs the standard Model Context Protocol (MCP) stdio server loop until EOF.
pub fn run_stdio_server() -> Result<(), String> {
    let server = McpServer::default();
    server.run_stdio()
}

/// Runs the MCP stdio server targeting a specific socket path.
pub fn run_stdio_server_with_socket(socket_path: Option<std::path::PathBuf>) -> Result<(), String> {
    let client = match socket_path {
        Some(p) => StrataDevClient::new(p),
        None => StrataDevClient::default(),
    };
    let server = McpServer::new(client);
    server.run_stdio()
}


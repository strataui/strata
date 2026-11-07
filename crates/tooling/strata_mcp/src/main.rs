//! Standalone Model Context Protocol (MCP) server binary for Strata.

use std::process;
use strata_mcp::run_stdio_server;

fn main() {
    if let Err(e) = run_stdio_server() {
        eprintln!("strata-mcp fatal error: {}", e);
        process::exit(1);
    }
}

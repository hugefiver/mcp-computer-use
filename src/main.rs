//! MCP Computer Use - Browser Control Server
//!
//! This MCP server provides browser control capabilities for AI models,
//! implementing Gemini computer use predefined tools using the thirtyfour
//! WebDriver library.
//!
//! # Configuration
//!
//! The server can be configured using environment variables:
//!
//! - `MCP_BROWSER_BINARY_PATH`: Path to the browser binary
//! - `MCP_WEBDRIVER_URL`: WebDriver server URL (default: http://localhost:9515)
//! - `MCP_BROWSER_TYPE`: Browser type (currently only `chrome` is supported)
//! - `MCP_SCREEN_WIDTH`: Screen width in pixels (default: 1280)
//! - `MCP_SCREEN_HEIGHT`: Screen height in pixels (default: 720)
//! - `MCP_INITIAL_URL`: Initial URL to load (default: https://www.google.com)
//! - `MCP_SEARCH_ENGINE_URL`: Search engine URL (default: https://www.google.com)
//! - `MCP_HEADLESS`: Run in headless mode (default: true)
//! - `MCP_DISABLED_TOOLS`: Comma-separated list of tools to disable
//! - `MCP_HIGHLIGHT_MOUSE`: Highlight mouse position for debugging (default: false)
//!
//! # Usage
//!
//! 1. Start a WebDriver server (e.g., ChromeDriver)
//! 2. Run this MCP server
//! 3. Connect an MCP client to interact with the browser

mod browser;
mod config;
mod tools;

use crate::config::Config;
use crate::tools::BrowserMcpServer;
use rmcp::transport::stdio;
use rmcp::ServiceExt;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("Starting MCP Computer Use server v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = Config::load()?;
    info!("Configuration loaded: {:?}", config);

    // Create the MCP server
    let server = BrowserMcpServer::new(config);

    // Run the server using stdio transport
    info!("Running MCP server on stdio...");
    let service = server.serve(stdio()).await?;

    // Wait for the service to complete
    service.waiting().await?;

    info!("MCP server shutting down");
    Ok(())
}

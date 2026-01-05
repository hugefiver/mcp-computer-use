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
//! - `MCP_TRANSPORT`: Transport mode: stdio or http (default: stdio)
//! - `MCP_HTTP_HOST`: HTTP server host (default: 127.0.0.1)
//! - `MCP_HTTP_PORT`: HTTP server port (default: 8080)
//! - `MCP_AUTO_LAUNCH_DRIVER`: Automatically launch browser driver (default: false)
//! - `MCP_DRIVER_PATH`: Path to browser driver executable
//! - `MCP_DRIVER_PORT`: Port for auto-launched driver (default: 9515)
//! - `MCP_UNDETECTED`: Enable undetected/stealth mode (default: false)
//! - `MCP_CONNECTION_MODE`: Connection mode: webdriver or cdp (default: webdriver)
//! - `MCP_CDP_PORT`: CDP port for direct browser connection (default: 9222)
//! - `MCP_AUTO_LAUNCH_BROWSER`: Automatically launch browser for CDP mode (default: false)
//! - `MCP_AUTO_DOWNLOAD_DRIVER`: Automatically download driver if not found (default: false)
//!
//! # Usage
//!
//! 1. Start a WebDriver server (e.g., ChromeDriver) or use MCP_AUTO_LAUNCH_DRIVER=true
//!    Or use CDP mode with MCP_CONNECTION_MODE=cdp and MCP_AUTO_LAUNCH_BROWSER=true
//! 2. Run this MCP server
//! 3. Connect an MCP client to interact with the browser

mod browser;
mod browser_manager;
mod config;
mod driver;
mod tools;

use crate::config::{Config, ConnectionMode, TransportMode};
use crate::driver::DriverManager;
use crate::tools::BrowserMcpServer;
use rmcp::transport::stdio;
use rmcp::ServiceExt;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[cfg(feature = "http-server")]
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    info!(
        "Starting MCP Computer Use server v{}",
        env!("CARGO_PKG_VERSION")
    );

    // Load configuration
    let mut config = Config::load()?;
    info!("Configuration loaded: {:?}", config);

    // Initialize driver manager (handles both WebDriver and CDP modes)
    let mut driver_manager = DriverManager::new();

    // Setup based on connection mode
    match config.connection_mode {
        ConnectionMode::WebDriver => {
            // Ensure driver is ready (launches if auto_launch_driver is enabled)
            match driver_manager.ensure_driver_ready(&config) {
                Ok(url) => {
                    if config.auto_launch_driver {
                        info!(
                            "Browser driver auto-launched, updating webdriver URL to: {}",
                            url
                        );
                    }
                    config.webdriver_url = url;
                }
                Err(e) => {
                    error!("Failed to ensure browser driver is ready: {}", e);
                    return Err(e);
                }
            }
        }
        ConnectionMode::Cdp => {
            info!("Using CDP (Chrome DevTools Protocol) mode");
            // If auto_launch_browser is enabled, launch browser with CDP
            if config.auto_launch_browser {
                match driver_manager
                    .browser_manager()
                    .launch_browser_with_cdp(&config)
                {
                    Ok(cdp_url) => {
                        info!("Browser launched with CDP at: {}", cdp_url);
                        // Update webdriver_url to point to CDP endpoint for thirtyfour
                        config.webdriver_url = cdp_url;
                    }
                    Err(e) => {
                        error!("Failed to launch browser with CDP: {}", e);
                        return Err(e);
                    }
                }
            } else {
                // Check if CDP endpoint is available
                if driver_manager
                    .browser_manager()
                    .is_cdp_available(config.cdp_port)
                {
                    info!(
                        "CDP endpoint available at port {}, using existing browser",
                        config.cdp_port
                    );
                    config.webdriver_url = format!("http://127.0.0.1:{}", config.cdp_port);
                } else {
                    warn!(
                        "CDP endpoint not available at port {}. Please start Chrome with \
                        --remote-debugging-port={} or set MCP_AUTO_LAUNCH_BROWSER=true",
                        config.cdp_port, config.cdp_port
                    );
                    // Fall back to auto-launching browser
                    info!("Auto-launching browser with CDP...");
                    match driver_manager
                        .browser_manager()
                        .launch_browser_with_cdp(&config)
                    {
                        Ok(cdp_url) => {
                            info!("Browser launched with CDP at: {}", cdp_url);
                            config.webdriver_url = cdp_url;
                        }
                        Err(e) => {
                            error!("Failed to launch browser with CDP: {}", e);
                            return Err(e);
                        }
                    }
                }
            }
        }
    }

    // Run server based on transport mode
    match config.transport_mode {
        TransportMode::Stdio => {
            run_stdio_server(config).await?;
        }
        TransportMode::Http => {
            #[cfg(feature = "http-server")]
            {
                run_http_server(config).await?;
            }
            #[cfg(not(feature = "http-server"))]
            {
                error!("HTTP transport not available. Build with 'http-server' feature enabled.");
                return Err(anyhow::anyhow!(
                    "HTTP transport requires 'http-server' feature"
                ));
            }
        }
    }

    // DriverManager is cleaned up automatically when it goes out of scope
    // (on any return path from this function, including errors).

    info!("MCP server shutting down");
    Ok(())
}

/// Run the MCP server using stdio transport.
async fn run_stdio_server(config: Config) -> anyhow::Result<()> {
    info!("Running MCP server on stdio...");

    let server = BrowserMcpServer::new(config);
    let service = server.serve(stdio()).await?;

    // Wait for the service to complete
    service.waiting().await?;

    Ok(())
}

/// Run the MCP server using HTTP streamable transport.
#[cfg(feature = "http-server")]
async fn run_http_server(config: Config) -> anyhow::Result<()> {
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    let bind_addr = format!("{}:{}", config.http_host, config.http_port);
    info!("Running MCP server on HTTP at {}...", bind_addr);

    // Security warning for non-localhost bindings
    if config.http_host != "127.0.0.1" && config.http_host != "localhost" {
        warn!(
            "⚠️  SECURITY WARNING: HTTP server is binding to '{}' which may expose the MCP endpoint \
            to the network. The HTTP endpoint has NO authentication. Only bind to non-localhost \
            addresses if you have proper security measures (TLS, authentication, firewall) in place.",
            config.http_host
        );
    }

    let config = Arc::new(config);

    let service: StreamableHttpService<BrowserMcpServer, LocalSessionManager> =
        StreamableHttpService::new(
            {
                let config = Arc::clone(&config);
                move || Ok(BrowserMcpServer::new_with_config(Arc::clone(&config)))
            },
            Default::default(),
            StreamableHttpServerConfig {
                stateful_mode: true,
                sse_keep_alive: Some(std::time::Duration::from_secs(15)),
            },
        );

    let router = axum::Router::new().nest_service("/mcp", service);

    let tcp_listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    info!("HTTP server listening on {}", bind_addr);

    let ct = CancellationToken::new();
    let ct_clone = ct.clone();

    // Handle Ctrl+C gracefully
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Received shutdown signal");
        ct_clone.cancel();
    });

    axum::serve(tcp_listener, router)
        .with_graceful_shutdown(async move { ct.cancelled().await })
        .await?;

    Ok(())
}

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
//! - `MCP_BROWSER_PATH`: Path to the browser binary (auto-detected if not set)
//! - `MCP_WEBDRIVER_URL`: WebDriver server URL (auto-determined when MCP_AUTO_START=true)
//! - `MCP_BROWSER_TYPE`: Browser type: `chrome`, `edge`, `firefox`, or `safari`
//! - `MCP_SCREEN_WIDTH`: Screen width in pixels (default: 1280)
//! - `MCP_SCREEN_HEIGHT`: Screen height in pixels (default: 720)
//! - `MCP_INITIAL_URL`: Initial URL to load (default: https://www.google.com)
//! - `MCP_SEARCH_ENGINE_URL`: Search engine URL (default: https://www.google.com)
//! - `MCP_HEADLESS`: Run in headless mode (default: true)
//! - `MCP_DISABLED_TOOLS`: Comma-separated list of tools to disable
//! - `MCP_TRANSPORT`: Transport mode: stdio or http (default: stdio)
//! - `MCP_HTTP_HOST`: HTTP server host (default: 127.0.0.1)
//! - `MCP_HTTP_PORT`: HTTP server port (default: 8080)
//! - `MCP_AUTO_START`: Automatically manage browser/driver lifecycle (default: false)
//! - `MCP_AUTO_DOWNLOAD_DRIVER`: Download driver if not found (default: false)
//! - `MCP_DRIVER_PATH`: Path to browser driver executable (auto-detected if not set)
//! - `MCP_DRIVER_PORT`: Port for driver (default: 9515)
//! - `MCP_UNDETECTED`: Enable undetected/stealth mode (default: false)
//! - `MCP_CONNECTION_MODE`: Connection mode: webdriver or cdp (default: webdriver)
//! - `MCP_CDP_PORT`: CDP port for browser connection (default: 9222)
//! - `MCP_OPEN_BROWSER_ON_START`: Open browser on MCP server startup (default: false)
//! - `MCP_IDLE_TIMEOUT`: Idle timeout duration (e.g., "10m", "30s", "0" to disable) (default: 10m)
//!
//! # Usage
//!
//! 1. Use MCP_AUTO_START=true for automatic driver/browser management
//! 2. Or manually start ChromeDriver and set MCP_WEBDRIVER_URL
//! 3. For CDP mode: set MCP_CONNECTION_MODE=cdp with MCP_AUTO_START=true
//! 4. Use MCP_OPEN_BROWSER_ON_START=true to pre-open browser on startup
//! 5. Run this MCP server and connect an MCP client

mod browser;
mod browser_manager;
mod cdp_browser;
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

    // Initialize driver manager (only for WebDriver mode)
    let mut driver_manager = DriverManager::new();

    // Setup based on connection mode
    match config.connection_mode {
        ConnectionMode::WebDriver => {
            // Ensure driver is ready (finds/downloads/launches if auto_start is enabled)
            match driver_manager.ensure_driver_ready(&config) {
                Ok(url) => {
                    if config.auto_start {
                        info!("Browser driver auto-started, using webdriver URL: {}", url);
                    }
                    config.webdriver_url = Some(url);
                }
                Err(e) => {
                    error!("Failed to ensure browser driver is ready: {}", e);
                    return Err(e);
                }
            }
        }
        ConnectionMode::Cdp => {
            // CDP mode uses direct CDP connection without WebDriver
            info!("Using CDP (Chrome DevTools Protocol) mode - no WebDriver required");
            let cdp_port = config.effective_cdp_port();

            if config.auto_start && config.open_browser_on_start {
                // Auto-start with open_browser_on_start: launch browser with CDP enabled now
                match driver_manager
                    .browser_manager()
                    .launch_browser_with_cdp(&config)
                {
                    Ok(cdp_url) => {
                        info!("Browser launched with CDP at: {}", cdp_url);
                        // Store CDP URL for later use - browser will be controlled directly via CDP
                        config.cdp_url = Some(cdp_url);
                    }
                    Err(e) => {
                        error!("Failed to launch browser with CDP: {}", e);
                        return Err(e);
                    }
                }
            } else if config.auto_start {
                // Auto-start without open_browser_on_start: browser will be launched on-demand
                // by CdpBrowserController when open_web_browser tool is called
                info!(
                    "CDP auto-start mode enabled, browser will be launched on-demand via open_web_browser tool"
                );
            } else {
                // Check if CDP endpoint is available (user started browser manually)
                if !driver_manager.browser_manager().is_cdp_available(cdp_port) {
                    return Err(anyhow::anyhow!(
                        "CDP endpoint not available at port {}. \
                         Please start Chrome with --remote-debugging-port={}, \
                         or enable MCP_AUTO_START=true to launch browser automatically.",
                        cdp_port,
                        cdp_port
                    ));
                }
                info!(
                    "CDP endpoint available at port {}, will connect to existing browser",
                    cdp_port
                );
                // Store CDP URL for connecting to the existing browser
                config.cdp_url = Some(format!("http://127.0.0.1:{}", cdp_port));
            }
            // No ChromeDriver needed in CDP mode - we use chromiumoxide directly
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
    info!("MCP server shutting down");
    Ok(())
}

/// Run the MCP server using stdio transport.
async fn run_stdio_server(config: Config) -> anyhow::Result<()> {
    info!("Running MCP server on stdio...");

    let server = BrowserMcpServer::new(config);

    // Initialize browser if open_browser_on_start is enabled
    server.init().await?;

    // Clone server for serve() since it takes ownership.
    // The clone shares the same Arc<BrowserBackend>, so shutdown() on either
    // reference will properly close the browser.
    let service = server.clone().serve(stdio()).await?;

    // Wait for the service to complete
    service.waiting().await?;

    // Always attempt to close the browser session gracefully on exit
    // This ensures the WebDriver/CDP session is properly closed
    if let Err(e) = server.shutdown().await {
        warn!("Error during browser shutdown: {}", e);
    }

    Ok(())
}

/// Run the MCP server using HTTP streamable transport.
#[cfg(feature = "http-server")]
async fn run_http_server(config: Config) -> anyhow::Result<()> {
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    let http_port = config.effective_http_port();
    let bind_addr = format!("{}:{}", config.http_host, http_port);
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

    // Warn about open_browser_on_start in HTTP mode
    if config.open_browser_on_start {
        warn!(
            "MCP_OPEN_BROWSER_ON_START is set, but in HTTP mode the browser is not automatically \
            opened on session start. This option is currently only effective in stdio mode. \
            Consider using stdio mode if you want the browser to be opened automatically."
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

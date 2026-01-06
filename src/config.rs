//! Configuration module for MCP Computer Use server.
//!
//! Supports configuration via environment variables and config files.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

/// Default WebDriver port (ChromeDriver default).
pub const DEFAULT_DRIVER_PORT: u16 = 9515;

/// Default CDP (Chrome DevTools Protocol) port.
pub const DEFAULT_CDP_PORT: u16 = 9222;

/// Default HTTP server port.
pub const DEFAULT_HTTP_PORT: u16 = 8080;

/// Transport mode for the MCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TransportMode {
    /// Standard input/output transport
    #[default]
    Stdio,
    /// HTTP streamable transport
    Http,
}

/// Browser connection mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionMode {
    /// WebDriver mode (requires a WebDriver server like ChromeDriver)
    #[default]
    WebDriver,
    /// Chrome DevTools Protocol mode (connects directly to browser's debug port)
    Cdp,
}

/// Main configuration for the MCP browser control server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Path to the browser binary (e.g., Chrome, Chromium, Firefox).
    /// If not set, the system will auto-detect the browser.
    pub browser_binary_path: Option<PathBuf>,

    /// WebDriver server URL (e.g., "http://localhost:9515" for ChromeDriver).
    /// If not set and auto_launch is false, defaults to "http://localhost:9515".
    /// If auto_launch is true, this is automatically determined.
    pub webdriver_url: Option<String>,

    /// Browser type to use.
    pub browser_type: BrowserType,

    /// Screen size configuration.
    pub screen_width: u32,
    pub screen_height: u32,

    /// Initial URL to navigate to when opening the browser.
    pub initial_url: String,

    /// Search engine URL for the search action.
    pub search_engine_url: String,

    /// Whether to run the browser in headless mode.
    pub headless: bool,

    /// Set of tool names to disable.
    pub disabled_tools: HashSet<String>,

    /// Whether to highlight mouse position (for debugging).
    pub highlight_mouse: bool,

    /// Transport mode: stdio or http.
    pub transport_mode: TransportMode,

    /// HTTP server port (only used when transport_mode is Http).
    /// If not set, defaults to 8080.
    pub http_port: Option<u16>,

    /// HTTP server host (only used when transport_mode is Http).
    pub http_host: String,

    /// Path to the browser driver executable.
    /// If not set, will try to find the driver in PATH or common locations,
    /// or download it if auto_download_driver is enabled.
    pub driver_path: Option<PathBuf>,

    /// Port to use for auto-launched driver.
    /// If not set, defaults to 9515.
    pub driver_port: Option<u16>,

    /// Whether to use undetected/stealth mode.
    pub undetected: bool,

    /// Browser connection mode: webdriver or cdp.
    pub connection_mode: ConnectionMode,

    /// CDP (Chrome DevTools Protocol) port for direct browser connection.
    /// Only used when connection_mode is Cdp.
    /// If not set, defaults to 9222.
    pub cdp_port: Option<u16>,

    /// Whether to automatically manage browser and driver lifecycle.
    /// When true:
    /// - In WebDriver mode: auto-launches ChromeDriver (downloads if needed)
    /// - In CDP mode: auto-launches browser with remote debugging enabled
    pub auto_start: bool,

    /// Whether to auto-download the browser driver if not found.
    /// Only effective when auto_start is true.
    pub auto_download_driver: bool,

    /// Whether to open browser on MCP server startup.
    /// When true, the browser will be opened automatically when the MCP server starts.
    /// Subsequent tool calls will use this pre-opened browser instance.
    /// Default is false (browser is opened only when open_web_browser tool is called).
    pub open_browser_on_start: bool,

    /// CDP URL for connecting to an existing browser.
    /// Set automatically when auto_start launches a browser with CDP,
    /// or can be derived from cdp_port when connecting to a manually started browser.
    pub cdp_url: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            browser_binary_path: None,
            webdriver_url: None, // Empty by default, determined at runtime
            browser_type: BrowserType::Chrome,
            screen_width: 1280,
            screen_height: 720,
            initial_url: "https://www.google.com".to_string(),
            search_engine_url: "https://www.google.com".to_string(),
            headless: true,
            disabled_tools: HashSet::new(),
            highlight_mouse: false,
            transport_mode: TransportMode::Stdio,
            http_port: None, // Fallback to DEFAULT_HTTP_PORT when needed
            http_host: "127.0.0.1".to_string(),
            driver_path: None,
            driver_port: None, // Fallback to DEFAULT_DRIVER_PORT when needed
            undetected: false,
            connection_mode: ConnectionMode::WebDriver,
            cdp_port: None, // Fallback to DEFAULT_CDP_PORT when needed
            auto_start: false,
            auto_download_driver: false,
            open_browser_on_start: false,
            cdp_url: None,
        }
    }
}

impl Config {
    /// Get the effective WebDriver URL.
    /// Returns the configured URL or falls back to default based on driver_port.
    pub fn effective_webdriver_url(&self) -> String {
        self.webdriver_url.clone().unwrap_or_else(|| {
            format!(
                "http://localhost:{}",
                self.driver_port.unwrap_or(DEFAULT_DRIVER_PORT)
            )
        })
    }

    /// Get the effective driver port.
    pub fn effective_driver_port(&self) -> u16 {
        self.driver_port.unwrap_or(DEFAULT_DRIVER_PORT)
    }

    /// Get the effective CDP port.
    pub fn effective_cdp_port(&self) -> u16 {
        self.cdp_port.unwrap_or(DEFAULT_CDP_PORT)
    }

    /// Get the effective HTTP port.
    pub fn effective_http_port(&self) -> u16 {
        self.http_port.unwrap_or(DEFAULT_HTTP_PORT)
    }
}

/// Supported browser types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BrowserType {
    #[default]
    Chrome,
    Firefox,
    Edge,
    Safari,
}

impl Config {
    /// Load configuration from environment variables and optional config file.
    pub fn load() -> anyhow::Result<Self> {
        let mut config = Config::default();

        // Load from environment variables
        if let Ok(path) = std::env::var("MCP_BROWSER_PATH") {
            config.browser_binary_path = Some(PathBuf::from(path));
        }

        if let Ok(url) = std::env::var("MCP_WEBDRIVER_URL") {
            config.webdriver_url = Some(url);
        }

        if let Ok(browser_type) = std::env::var("MCP_BROWSER_TYPE") {
            config.browser_type = match browser_type.to_lowercase().as_str() {
                "chrome" => BrowserType::Chrome,
                "firefox" => BrowserType::Firefox,
                "edge" => BrowserType::Edge,
                "safari" => BrowserType::Safari,
                _ => BrowserType::Chrome,
            };
        }

        if let Ok(width) = std::env::var("MCP_SCREEN_WIDTH") {
            config.screen_width = match width.parse() {
                Ok(w) => w,
                Err(e) => {
                    tracing::warn!(
                        "Invalid MCP_SCREEN_WIDTH '{}': {}, using default 1280",
                        width,
                        e
                    );
                    1280
                }
            };
        }

        if let Ok(height) = std::env::var("MCP_SCREEN_HEIGHT") {
            config.screen_height = match height.parse() {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!(
                        "Invalid MCP_SCREEN_HEIGHT '{}': {}, using default 720",
                        height,
                        e
                    );
                    720
                }
            };
        }

        if let Ok(url) = std::env::var("MCP_INITIAL_URL") {
            config.initial_url = url;
        }

        if let Ok(url) = std::env::var("MCP_SEARCH_ENGINE_URL") {
            config.search_engine_url = url;
        }

        if let Ok(headless) = std::env::var("MCP_HEADLESS") {
            config.headless = match headless.to_lowercase().as_str() {
                "true" | "1" | "yes" => true,
                "false" | "0" | "no" => false,
                _ => {
                    tracing::warn!("Invalid MCP_HEADLESS '{}', using default true", headless);
                    true
                }
            };
        }

        if let Ok(disabled) = std::env::var("MCP_DISABLED_TOOLS") {
            config.disabled_tools = disabled
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }

        if let Ok(highlight) = std::env::var("MCP_HIGHLIGHT_MOUSE") {
            config.highlight_mouse = match highlight.to_lowercase().as_str() {
                "true" | "1" | "yes" => true,
                "false" | "0" | "no" => false,
                _ => {
                    tracing::warn!(
                        "Invalid MCP_HIGHLIGHT_MOUSE '{}', using default false",
                        highlight
                    );
                    false
                }
            };
        }

        // Transport configuration
        if let Ok(transport) = std::env::var("MCP_TRANSPORT") {
            config.transport_mode = match transport.to_lowercase().as_str() {
                "http" => TransportMode::Http,
                "stdio" => TransportMode::Stdio,
                _ => {
                    tracing::warn!("Invalid MCP_TRANSPORT '{}', using default stdio", transport);
                    TransportMode::Stdio
                }
            };
        }

        if let Ok(port) = std::env::var("MCP_HTTP_PORT") {
            config.http_port = match port.parse() {
                Ok(p) => Some(p),
                Err(e) => {
                    tracing::warn!("Invalid MCP_HTTP_PORT '{}': {}, will use default", port, e);
                    None
                }
            };
        }

        if let Ok(host) = std::env::var("MCP_HTTP_HOST") {
            config.http_host = host;
        }

        if let Ok(path) = std::env::var("MCP_DRIVER_PATH") {
            config.driver_path = Some(PathBuf::from(path));
        }

        if let Ok(port) = std::env::var("MCP_DRIVER_PORT") {
            config.driver_port = match port.parse() {
                Ok(p) => Some(p),
                Err(e) => {
                    tracing::warn!(
                        "Invalid MCP_DRIVER_PORT '{}': {}, will use default",
                        port,
                        e
                    );
                    None
                }
            };
        }

        // Undetected mode configuration
        if let Ok(undetected) = std::env::var("MCP_UNDETECTED") {
            config.undetected = match undetected.to_lowercase().as_str() {
                "true" | "1" | "yes" => true,
                "false" | "0" | "no" => false,
                _ => {
                    tracing::warn!(
                        "Invalid MCP_UNDETECTED '{}', using default false",
                        undetected
                    );
                    false
                }
            };
        }

        // Connection mode configuration
        if let Ok(mode) = std::env::var("MCP_CONNECTION_MODE") {
            config.connection_mode = match mode.to_lowercase().as_str() {
                "cdp" => ConnectionMode::Cdp,
                "webdriver" => ConnectionMode::WebDriver,
                _ => {
                    tracing::warn!(
                        "Invalid MCP_CONNECTION_MODE '{}', using default webdriver",
                        mode
                    );
                    ConnectionMode::WebDriver
                }
            };
        }

        // CDP port configuration
        if let Ok(port) = std::env::var("MCP_CDP_PORT") {
            config.cdp_port = match port.parse() {
                Ok(p) => Some(p),
                Err(e) => {
                    tracing::warn!("Invalid MCP_CDP_PORT '{}': {}, will use default", port, e);
                    None
                }
            };
        }

        // Auto-start configuration (unified flag for both driver and browser)
        if let Ok(auto_start) = std::env::var("MCP_AUTO_START") {
            config.auto_start = match auto_start.to_lowercase().as_str() {
                "true" | "1" | "yes" => true,
                "false" | "0" | "no" => false,
                _ => {
                    tracing::warn!(
                        "Invalid MCP_AUTO_START '{}', using default false",
                        auto_start
                    );
                    false
                }
            };
        }

        // Auto-download driver configuration
        if let Ok(auto_download) = std::env::var("MCP_AUTO_DOWNLOAD_DRIVER") {
            config.auto_download_driver = match auto_download.to_lowercase().as_str() {
                "true" | "1" | "yes" => true,
                "false" | "0" | "no" => false,
                _ => {
                    tracing::warn!(
                        "Invalid MCP_AUTO_DOWNLOAD_DRIVER '{}', using default false",
                        auto_download
                    );
                    false
                }
            };
        }

        // Open browser on start configuration
        if let Ok(open_on_start) = std::env::var("MCP_OPEN_BROWSER_ON_START") {
            config.open_browser_on_start = match open_on_start.to_lowercase().as_str() {
                "true" | "1" | "yes" => true,
                "false" | "0" | "no" => false,
                _ => {
                    tracing::warn!(
                        "Invalid MCP_OPEN_BROWSER_ON_START '{}', using default false",
                        open_on_start
                    );
                    false
                }
            };
        }

        // Validate browser type - only Chrome is currently supported
        if config.browser_type != BrowserType::Chrome {
            anyhow::bail!(
                "Only Chrome browser is currently supported. Got: {:?}. \
                Please set MCP_BROWSER_TYPE=chrome or remove the environment variable.",
                config.browser_type
            );
        }

        Ok(config)
    }

    /// Check if a tool is disabled.
    pub fn is_tool_disabled(&self, tool_name: &str) -> bool {
        self.disabled_tools.contains(tool_name)
    }
}

/// All available tool names for reference.
pub mod tool_names {
    pub const CLICK_AT: &str = "click_at";
    pub const HOVER_AT: &str = "hover_at";
    pub const TYPE_TEXT_AT: &str = "type_text_at";
    pub const SCROLL_DOCUMENT: &str = "scroll_document";
    pub const SCROLL_AT: &str = "scroll_at";
    pub const WAIT_5_SECONDS: &str = "wait_5_seconds";
    pub const GO_BACK: &str = "go_back";
    pub const GO_FORWARD: &str = "go_forward";
    pub const SEARCH: &str = "search";
    pub const NAVIGATE: &str = "navigate";
    pub const KEY_COMBINATION: &str = "key_combination";
    pub const DRAG_AND_DROP: &str = "drag_and_drop";
    pub const CURRENT_STATE: &str = "current_state";
    pub const OPEN_WEB_BROWSER: &str = "open_web_browser";
    // Tab operations
    pub const NEW_TAB: &str = "new_tab";
    pub const CLOSE_TAB: &str = "close_tab";
    pub const SWITCH_TAB: &str = "switch_tab";
    pub const LIST_TABS: &str = "list_tabs";
}

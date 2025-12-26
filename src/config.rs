//! Configuration module for MCP Computer Use server.
//!
//! Supports configuration via environment variables and config files.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

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

/// Main configuration for the MCP browser control server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Path to the browser binary (e.g., Chrome, Chromium, Firefox).
    /// If not set, the default system browser will be used.
    pub browser_binary_path: Option<PathBuf>,

    /// WebDriver server URL (e.g., "http://localhost:9515" for ChromeDriver).
    /// If not set, defaults to "http://localhost:9515".
    pub webdriver_url: String,

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
    pub http_port: u16,

    /// HTTP server host (only used when transport_mode is Http).
    pub http_host: String,

    /// Whether to auto-launch the browser driver.
    pub auto_launch_driver: bool,

    /// Path to the browser driver executable.
    /// If not set, will try to find the driver in PATH.
    pub driver_path: Option<PathBuf>,

    /// Port to use for auto-launched driver.
    pub driver_port: u16,

    /// Whether to use undetected/stealth mode.
    pub undetected: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            browser_binary_path: None,
            webdriver_url: "http://localhost:9515".to_string(),
            browser_type: BrowserType::Chrome,
            screen_width: 1280,
            screen_height: 720,
            initial_url: "https://www.google.com".to_string(),
            search_engine_url: "https://www.google.com".to_string(),
            headless: true,
            disabled_tools: HashSet::new(),
            highlight_mouse: false,
            transport_mode: TransportMode::Stdio,
            http_port: 8080,
            http_host: "127.0.0.1".to_string(),
            auto_launch_driver: false,
            driver_path: None,
            driver_port: 9515,
            undetected: false,
        }
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
        if let Ok(path) = std::env::var("MCP_BROWSER_BINARY_PATH") {
            config.browser_binary_path = Some(PathBuf::from(path));
        }

        if let Ok(url) = std::env::var("MCP_WEBDRIVER_URL") {
            config.webdriver_url = url;
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
            config.headless = match headless.parse() {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!(
                        "Invalid MCP_HEADLESS '{}': {}, using default true",
                        headless,
                        e
                    );
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
            config.highlight_mouse = match highlight.parse() {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!(
                        "Invalid MCP_HIGHLIGHT_MOUSE '{}': {}, using default false",
                        highlight,
                        e
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
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        "Invalid MCP_HTTP_PORT '{}': {}, using default 8080",
                        port,
                        e
                    );
                    8080
                }
            };
        }

        if let Ok(host) = std::env::var("MCP_HTTP_HOST") {
            config.http_host = host;
        }

        // Auto-launch driver configuration
        if let Ok(auto_launch) = std::env::var("MCP_AUTO_LAUNCH_DRIVER") {
            config.auto_launch_driver = match auto_launch.parse() {
                Ok(a) => a,
                Err(e) => {
                    tracing::warn!(
                        "Invalid MCP_AUTO_LAUNCH_DRIVER '{}': {}, using default false",
                        auto_launch,
                        e
                    );
                    false
                }
            };
        }

        if let Ok(path) = std::env::var("MCP_DRIVER_PATH") {
            config.driver_path = Some(PathBuf::from(path));
        }

        if let Ok(port) = std::env::var("MCP_DRIVER_PORT") {
            config.driver_port = match port.parse() {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        "Invalid MCP_DRIVER_PORT '{}': {}, using default 9515",
                        port,
                        e
                    );
                    9515
                }
            };
        }

        // Undetected mode configuration
        if let Ok(undetected) = std::env::var("MCP_UNDETECTED") {
            config.undetected = match undetected.parse() {
                Ok(u) => u,
                Err(e) => {
                    tracing::warn!(
                        "Invalid MCP_UNDETECTED '{}': {}, using default false",
                        undetected,
                        e
                    );
                    false
                }
            };
        }

        // Validate browser type - only Chrome is currently supported
        if config.browser_type != BrowserType::Chrome {
            panic!(
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

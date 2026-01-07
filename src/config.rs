//! Configuration module for MCP Computer Use server.
//!
//! Supports configuration via environment variables and config files.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

/// Default WebDriver port (ChromeDriver default).
pub const DEFAULT_DRIVER_PORT: u16 = 9515;

/// Default CDP (Chrome DevTools Protocol) port.
pub const DEFAULT_CDP_PORT: u16 = 9222;

/// Default HTTP server port.
pub const DEFAULT_HTTP_PORT: u16 = 8080;

/// Parse a duration string into a Duration.
///
/// Accepts formats like:
/// - "10m" (10 minutes)
/// - "5s" (5 seconds)
/// - "1h" (1 hour)
/// - "0" or "0s" (disable - returns Duration::ZERO)
/// - Plain number (interpreted as seconds)
fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim().to_lowercase();

    if s.is_empty() {
        return Err("Empty duration string".to_string());
    }

    // Check for disable case
    if s == "0" {
        return Ok(Duration::ZERO);
    }

    // Try to parse with suffix
    let (num_str, multiplier) = if s.ends_with('s') {
        (&s[..s.len() - 1], 1u64)
    } else if s.ends_with('m') {
        (&s[..s.len() - 1], 60u64)
    } else if s.ends_with('h') {
        (&s[..s.len() - 1], 3600u64)
    } else {
        // Assume seconds if no suffix
        (s.as_str(), 1u64)
    };

    let num: u64 = num_str
        .parse()
        .map_err(|e| format!("Invalid number '{}': {}", num_str, e))?;

    let seconds = num
        .checked_mul(multiplier)
        .ok_or_else(|| format!("Duration overflow for value '{}'", s))?;

    Ok(Duration::from_secs(seconds))
}

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

    /// Idle timeout duration for automatically closing the browser when inactive.
    /// After this duration of no operations, the browser will be closed automatically.
    /// Set to 0 (or Duration::ZERO) to disable idle timeout.
    /// Default is 10 minutes.
    pub idle_timeout: std::time::Duration,
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
            idle_timeout: std::time::Duration::from_secs(600), // 10 minutes default
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

        // Idle timeout configuration
        // Accepts duration strings like "10m", "5s", "1h", "0" (disable), or plain seconds
        if let Ok(timeout_str) = std::env::var("MCP_IDLE_TIMEOUT") {
            config.idle_timeout = parse_duration(&timeout_str).unwrap_or_else(|e| {
                tracing::warn!(
                    "Invalid MCP_IDLE_TIMEOUT '{}': {}, using default 10m",
                    timeout_str,
                    e
                );
                std::time::Duration::from_secs(600)
            });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_seconds() {
        assert_eq!(parse_duration("30").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("60S").unwrap(), Duration::from_secs(60));
    }

    #[test]
    fn test_parse_duration_minutes() {
        assert_eq!(parse_duration("10m").unwrap(), Duration::from_secs(600));
        assert_eq!(parse_duration("5M").unwrap(), Duration::from_secs(300));
    }

    #[test]
    fn test_parse_duration_hours() {
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration("2H").unwrap(), Duration::from_secs(7200));
    }

    #[test]
    fn test_parse_duration_zero() {
        assert_eq!(parse_duration("0").unwrap(), Duration::ZERO);
        assert_eq!(parse_duration("0s").unwrap(), Duration::ZERO);
        assert_eq!(parse_duration("0m").unwrap(), Duration::ZERO);
    }

    #[test]
    fn test_parse_duration_whitespace() {
        assert_eq!(parse_duration("  10m  ").unwrap(), Duration::from_secs(600));
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("10x").is_err());
    }

    #[test]
    fn test_parse_duration_overflow() {
        // Very large number should return an overflow error
        assert!(parse_duration("99999999999999999999999h").is_err());
    }
}

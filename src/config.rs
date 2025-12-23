//! Configuration module for MCP Computer Use server.
//!
//! Supports configuration via environment variables and config files.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

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
        }
    }
}

/// Supported browser types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowserType {
    Chrome,
    Firefox,
    Edge,
    Safari,
}

impl Default for BrowserType {
    fn default() -> Self {
        Self::Chrome
    }
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
            config.screen_width = width.parse().unwrap_or(1280);
        }

        if let Ok(height) = std::env::var("MCP_SCREEN_HEIGHT") {
            config.screen_height = height.parse().unwrap_or(720);
        }

        if let Ok(url) = std::env::var("MCP_INITIAL_URL") {
            config.initial_url = url;
        }

        if let Ok(url) = std::env::var("MCP_SEARCH_ENGINE_URL") {
            config.search_engine_url = url;
        }

        if let Ok(headless) = std::env::var("MCP_HEADLESS") {
            config.headless = headless.parse().unwrap_or(true);
        }

        if let Ok(disabled) = std::env::var("MCP_DISABLED_TOOLS") {
            config.disabled_tools = disabled
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }

        if let Ok(highlight) = std::env::var("MCP_HIGHLIGHT_MOUSE") {
            config.highlight_mouse = highlight.parse().unwrap_or(false);
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
}

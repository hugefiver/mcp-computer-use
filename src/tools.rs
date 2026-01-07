//! MCP Tools implementation for browser control.
//!
//! This module defines all the MCP tools that expose browser control capabilities.

use crate::browser::{BrowserController, EnvState, TabInfo};
use crate::cdp_browser::CdpBrowserController;
use crate::config::{tool_names, Config, ConnectionMode};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, ErrorData as McpError, Implementation, ServerCapabilities,
        ServerInfo,
    },
    schemars, tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Unified browser interface that supports both WebDriver and CDP modes.
pub enum BrowserBackend {
    WebDriver(Arc<BrowserController>),
    Cdp(Arc<CdpBrowserController>),
}

impl BrowserBackend {
    /// Create a new browser backend based on connection mode.
    pub fn new(config: Config) -> Self {
        match config.connection_mode {
            ConnectionMode::WebDriver => {
                BrowserBackend::WebDriver(Arc::new(BrowserController::new(config)))
            }
            ConnectionMode::Cdp => BrowserBackend::Cdp(Arc::new(CdpBrowserController::new(config))),
        }
    }

    /// Open the browser.
    pub async fn open(&self) -> anyhow::Result<EnvState> {
        match self {
            BrowserBackend::WebDriver(ctrl) => ctrl.open().await,
            BrowserBackend::Cdp(ctrl) => ctrl.open().await,
        }
    }

    /// Get current state.
    pub async fn current_state(&self) -> anyhow::Result<EnvState> {
        match self {
            BrowserBackend::WebDriver(ctrl) => ctrl.current_state().await,
            BrowserBackend::Cdp(ctrl) => ctrl.current_state().await,
        }
    }

    /// Click at coordinates.
    pub async fn click_at(&self, x: i64, y: i64) -> anyhow::Result<EnvState> {
        match self {
            BrowserBackend::WebDriver(ctrl) => ctrl.click_at(x, y).await,
            BrowserBackend::Cdp(ctrl) => ctrl.click_at(x, y).await,
        }
    }

    /// Hover at coordinates.
    pub async fn hover_at(&self, x: i64, y: i64) -> anyhow::Result<EnvState> {
        match self {
            BrowserBackend::WebDriver(ctrl) => ctrl.hover_at(x, y).await,
            BrowserBackend::Cdp(ctrl) => ctrl.hover_at(x, y).await,
        }
    }

    /// Type text at coordinates.
    pub async fn type_text_at(
        &self,
        x: i64,
        y: i64,
        text: &str,
        press_enter: bool,
        clear_before_typing: bool,
    ) -> anyhow::Result<EnvState> {
        match self {
            BrowserBackend::WebDriver(ctrl) => {
                ctrl.type_text_at(x, y, text, press_enter, clear_before_typing)
                    .await
            }
            BrowserBackend::Cdp(ctrl) => {
                ctrl.type_text_at(x, y, text, press_enter, clear_before_typing)
                    .await
            }
        }
    }

    /// Scroll the document.
    pub async fn scroll_document(&self, direction: &str) -> anyhow::Result<EnvState> {
        match self {
            BrowserBackend::WebDriver(ctrl) => ctrl.scroll_document(direction).await,
            BrowserBackend::Cdp(ctrl) => ctrl.scroll_document(direction).await,
        }
    }

    /// Scroll at coordinates.
    pub async fn scroll_at(
        &self,
        x: i64,
        y: i64,
        direction: &str,
        magnitude: i64,
    ) -> anyhow::Result<EnvState> {
        match self {
            BrowserBackend::WebDriver(ctrl) => ctrl.scroll_at(x, y, direction, magnitude).await,
            BrowserBackend::Cdp(ctrl) => ctrl.scroll_at(x, y, direction, magnitude).await,
        }
    }

    /// Wait 5 seconds.
    pub async fn wait_5_seconds(&self) -> anyhow::Result<EnvState> {
        match self {
            BrowserBackend::WebDriver(ctrl) => ctrl.wait_5_seconds().await,
            BrowserBackend::Cdp(ctrl) => ctrl.wait_5_seconds().await,
        }
    }

    /// Go back.
    pub async fn go_back(&self) -> anyhow::Result<EnvState> {
        match self {
            BrowserBackend::WebDriver(ctrl) => ctrl.go_back().await,
            BrowserBackend::Cdp(ctrl) => ctrl.go_back().await,
        }
    }

    /// Go forward.
    pub async fn go_forward(&self) -> anyhow::Result<EnvState> {
        match self {
            BrowserBackend::WebDriver(ctrl) => ctrl.go_forward().await,
            BrowserBackend::Cdp(ctrl) => ctrl.go_forward().await,
        }
    }

    /// Search.
    pub async fn search(&self) -> anyhow::Result<EnvState> {
        match self {
            BrowserBackend::WebDriver(ctrl) => ctrl.search().await,
            BrowserBackend::Cdp(ctrl) => ctrl.search().await,
        }
    }

    /// Navigate.
    pub async fn navigate(&self, url: &str) -> anyhow::Result<EnvState> {
        match self {
            BrowserBackend::WebDriver(ctrl) => ctrl.navigate(url).await,
            BrowserBackend::Cdp(ctrl) => ctrl.navigate(url).await,
        }
    }

    /// Key combination.
    pub async fn key_combination(&self, keys: Vec<String>) -> anyhow::Result<EnvState> {
        match self {
            BrowserBackend::WebDriver(ctrl) => ctrl.key_combination(keys).await,
            BrowserBackend::Cdp(ctrl) => ctrl.key_combination(keys).await,
        }
    }

    /// Drag and drop.
    pub async fn drag_and_drop(
        &self,
        x: i64,
        y: i64,
        destination_x: i64,
        destination_y: i64,
    ) -> anyhow::Result<EnvState> {
        match self {
            BrowserBackend::WebDriver(ctrl) => {
                ctrl.drag_and_drop(x, y, destination_x, destination_y).await
            }
            BrowserBackend::Cdp(ctrl) => {
                ctrl.drag_and_drop(x, y, destination_x, destination_y).await
            }
        }
    }

    /// New tab (WebDriver only).
    pub async fn new_tab(&self, url: Option<&str>) -> anyhow::Result<(TabInfo, EnvState)> {
        match self {
            BrowserBackend::WebDriver(ctrl) => ctrl.new_tab(url).await,
            BrowserBackend::Cdp(_) => Err(anyhow::anyhow!(
                "Tab management is not supported in CDP mode. Use WebDriver mode for tab operations."
            )),
        }
    }

    /// Close tab (WebDriver only).
    pub async fn close_tab(&self, handle: Option<&str>) -> anyhow::Result<EnvState> {
        match self {
            BrowserBackend::WebDriver(ctrl) => ctrl.close_tab(handle).await,
            BrowserBackend::Cdp(_) => Err(anyhow::anyhow!(
                "Tab management is not supported in CDP mode. Use WebDriver mode for tab operations."
            )),
        }
    }

    /// Switch tab (WebDriver only).
    pub async fn switch_tab(
        &self,
        handle: Option<&str>,
        index: Option<usize>,
    ) -> anyhow::Result<EnvState> {
        match self {
            BrowserBackend::WebDriver(ctrl) => ctrl.switch_tab(handle, index).await,
            BrowserBackend::Cdp(_) => Err(anyhow::anyhow!(
                "Tab management is not supported in CDP mode. Use WebDriver mode for tab operations."
            )),
        }
    }

    /// List tabs (WebDriver only).
    pub async fn list_tabs(&self) -> anyhow::Result<(Vec<TabInfo>, EnvState)> {
        match self {
            BrowserBackend::WebDriver(ctrl) => ctrl.list_tabs().await,
            BrowserBackend::Cdp(_) => Err(anyhow::anyhow!(
                "Tab management is not supported in CDP mode. Use WebDriver mode for tab operations."
            )),
        }
    }

    /// Close the browser and clean up resources.
    pub async fn close(&self) -> anyhow::Result<()> {
        match self {
            BrowserBackend::WebDriver(ctrl) => ctrl.close().await,
            BrowserBackend::Cdp(ctrl) => ctrl.close().await,
        }
    }
}

/// Response type for browser actions that includes screenshot and URL.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BrowserStateResponse {
    /// Current URL of the page.
    pub url: String,
    /// Whether the action was successful.
    pub success: bool,
    /// Optional message describing the result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

fn env_state_to_result(state: EnvState, message: Option<&str>) -> Result<CallToolResult, McpError> {
    let response = BrowserStateResponse {
        url: state.url,
        success: true,
        message: message.map(String::from),
    };
    let text = serde_json::to_string_pretty(&response)
        .or_else(|_| serde_json::to_string(&response))
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to serialize response: {}", e);
            // Fallback: construct minimal valid JSON with safely escaped URL
            let safe_url =
                serde_json::to_string(&response.url).unwrap_or_else(|_| "null".to_string());
            format!(r#"{{"url":{},"success":true}}"#, safe_url)
        });
    let text_content = Content::text(text);
    let image_content = Content::image(state.screenshot, "image/png");

    Ok(CallToolResult::success(vec![text_content, image_content]))
}

fn error_to_result(error: &str) -> Result<CallToolResult, McpError> {
    let response = BrowserStateResponse {
        url: String::new(),
        success: false,
        message: Some(error.to_string()),
    };
    // Use serde_json without pretty printing as fallback since it's more reliable
    let text = serde_json::to_string_pretty(&response)
        .or_else(|_| serde_json::to_string(&response))
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to serialize error response: {}", e);
            // Construct minimal valid JSON manually
            format!(
                r#"{{"success":false,"message":"{}"}}"#,
                error
                    .chars()
                    .filter(|c| c.is_ascii() && *c != '"' && *c != '\\')
                    .collect::<String>()
            )
        });
    Ok(CallToolResult::error(vec![Content::text(text)]))
}

/// Returns an MCP-level error for disabled tools.
fn disabled_tool_error(tool_name: &str) -> Result<CallToolResult, McpError> {
    Err(McpError::invalid_request(
        format!(
            "Tool '{}' is disabled via MCP_DISABLED_TOOLS configuration",
            tool_name
        ),
        None,
    ))
}

/// MCP Server handler for browser control.
#[derive(Clone)]
pub struct BrowserMcpServer {
    browser: Arc<BrowserBackend>,
    config: Arc<Config>,
    tool_router: ToolRouter<Self>,
    /// Timestamp of last activity (seconds since UNIX epoch).
    /// Used for idle timeout tracking.
    last_activity: Arc<AtomicU64>,
    /// Handle to the idle timeout monitor task.
    /// Used to manage the task lifecycle; the task is explicitly cancelled (via `abort`) during shutdown.
    idle_monitor_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Flag to indicate that a browser operation is currently in progress.
    /// Used to prevent the idle timeout from closing the browser during active operations.
    operation_in_progress: Arc<AtomicBool>,
}

impl BrowserMcpServer {
    /// Create a new MCP server with the given configuration.
    pub fn new(config: Config) -> Self {
        let config = Arc::new(config);
        Self::new_with_config(config)
    }

    /// Create a new MCP server with an Arc-wrapped configuration.
    /// This avoids cloning the config for each session in HTTP mode.
    pub fn new_with_config(config: Arc<Config>) -> Self {
        let browser = Arc::new(BrowserBackend::new((*config).clone()));
        let last_activity = Arc::new(AtomicU64::new(current_timestamp()));
        Self {
            browser,
            config,
            tool_router: Self::tool_router(),
            last_activity,
            idle_monitor_handle: Arc::new(Mutex::new(None)),
            operation_in_progress: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Update the last activity timestamp and mark operation as in progress.
    /// Note: The two atomic stores are not atomic as a unit. A reader could see
    /// `operation_in_progress=true` but the old `last_activity` timestamp if it reads
    /// between the two stores. This is acceptable for idle timeout tracking since
    /// the monitor will simply wait for the next check interval.
    fn touch(&self) {
        self.operation_in_progress.store(true, Ordering::Release);
        self.last_activity
            .store(current_timestamp(), Ordering::Release);
    }

    /// Mark the current operation as complete.
    fn operation_complete(&self) {
        // Update timestamp first to ensure accurate idle tracking
        self.last_activity
            .store(current_timestamp(), Ordering::Release);
        self.operation_in_progress.store(false, Ordering::Release);
    }

    /// Get the duration since last activity.
    #[allow(dead_code)]
    fn idle_duration(&self) -> Duration {
        let last = self.last_activity.load(Ordering::Acquire);
        let now = current_timestamp();
        Duration::from_secs(now.saturating_sub(last))
    }

    /// Start the idle timeout monitor if configured.
    /// This spawns a background task that closes the browser after idle timeout.
    /// If a monitor is already running, this function does nothing.
    pub async fn start_idle_monitor(&self) {
        let idle_timeout = self.config.idle_timeout;

        // If idle timeout is zero, don't start the monitor
        if idle_timeout.is_zero() {
            debug!("Idle timeout is disabled (set to 0)");
            return;
        }

        // Check if a monitor is already running
        {
            let guard = self.idle_monitor_handle.lock().await;
            if guard.is_some() {
                debug!("Idle monitor is already running, skipping start");
                return;
            }
        }

        let browser = Arc::clone(&self.browser);
        let last_activity = Arc::clone(&self.last_activity);
        let operation_in_progress = Arc::clone(&self.operation_in_progress);
        // Check 4 times per timeout period, but at least once per second
        // to avoid excessive polling for very short timeouts
        let check_interval = (idle_timeout / 4).max(Duration::from_secs(1));

        let handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(check_interval).await;

                // Check if an operation is currently in progress first
                if operation_in_progress.load(Ordering::Acquire) {
                    continue;
                }

                let last = last_activity.load(Ordering::Acquire);
                let now = current_timestamp();
                let idle_secs = now.saturating_sub(last);

                // Check idle time and verify no operation started
                if idle_secs >= idle_timeout.as_secs()
                    && !operation_in_progress.load(Ordering::Acquire)
                {
                    // Set operation_in_progress to prevent new operations from starting
                    // while we're closing the browser
                    operation_in_progress.store(true, Ordering::Release);

                    info!(
                        "Browser idle for {}s (timeout: {}s), closing browser",
                        idle_secs,
                        idle_timeout.as_secs()
                    );
                    if let Err(e) = browser.close().await {
                        warn!("Error closing browser due to idle timeout: {}", e);
                    }

                    // Clear the flag after closing
                    operation_in_progress.store(false, Ordering::Release);
                    break;
                }
            }
        });

        let mut guard = self.idle_monitor_handle.lock().await;
        *guard = Some(handle);
    }

    /// Initialize the server, optionally opening the browser if configured.
    /// Call this after construction if `open_browser_on_start` is enabled.
    pub async fn init(&self) -> anyhow::Result<()> {
        if self.config.open_browser_on_start {
            info!("Opening browser on server start (MCP_OPEN_BROWSER_ON_START=true)");
            // Note: touch() and start_idle_monitor() are only called if open() succeeds
            // due to the ? operator returning early on error
            self.browser.open().await?;
            self.touch();
            self.operation_complete();
            // Start idle monitor only after browser is actually opened
            self.start_idle_monitor().await;
        }

        Ok(())
    }

    /// Shutdown the server and close the browser.
    /// This should be called before the program exits when auto_start is enabled
    /// to ensure the browser is properly closed.
    pub async fn shutdown(&self) -> anyhow::Result<()> {
        info!("Shutting down MCP server, closing browser...");

        // Cancel idle monitor if running
        let mut guard = self.idle_monitor_handle.lock().await;
        if let Some(handle) = guard.take() {
            handle.abort();
        }
        drop(guard);

        self.browser.close().await
    }

    /// Get a reference to the browser backend.
    #[allow(dead_code)]
    pub fn browser(&self) -> &Arc<BrowserBackend> {
        &self.browser
    }
}

/// Get the current timestamp in seconds since UNIX epoch.
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time should be after UNIX epoch")
        .as_secs()
}

// Tool parameter types
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClickAtParams {
    /// X coordinate on the screen.
    pub x: i64,
    /// Y coordinate on the screen.
    pub y: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HoverAtParams {
    /// X coordinate on the screen.
    pub x: i64,
    /// Y coordinate on the screen.
    pub y: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TypeTextAtParams {
    /// X coordinate on the screen.
    pub x: i64,
    /// Y coordinate on the screen.
    pub y: i64,
    /// Text to type.
    pub text: String,
    /// Whether to press Enter after typing. Defaults to false.
    #[serde(default)]
    pub press_enter: bool,
    /// Whether to clear existing content before typing. Defaults to true.
    #[serde(default = "default_true")]
    pub clear_before_typing: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScrollDocumentParams {
    /// Direction to scroll: "up", "down", "left", or "right".
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScrollAtParams {
    /// X coordinate on the screen.
    pub x: i64,
    /// Y coordinate on the screen.
    pub y: i64,
    /// Direction to scroll: "up", "down", "left", or "right".
    pub direction: String,
    /// Magnitude of scroll in pixels. Defaults to 800.
    #[serde(default = "default_magnitude")]
    pub magnitude: i64,
}

fn default_magnitude() -> i64 {
    800
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NavigateParams {
    /// URL to navigate to. Will be prefixed with "https://" if no protocol specified.
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct KeyCombinationParams {
    /// List of keys to press together. Example: ["Control", "c"] for Ctrl+C.
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DragAndDropParams {
    /// Starting X coordinate.
    pub x: i64,
    /// Starting Y coordinate.
    pub y: i64,
    /// Destination X coordinate.
    pub destination_x: i64,
    /// Destination Y coordinate.
    pub destination_y: i64,
}

// Tab operation parameter types
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NewTabParams {
    /// Optional URL to navigate to in the new tab.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CloseTabParams {
    /// The window handle of the tab to close. If not provided, closes the current tab.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
}

/// Parameters for switching to a tab.
/// Exactly one of `handle` or `index` must be provided.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct SwitchTabParams {
    /// The window handle of the tab to switch to.
    /// Exactly one of `handle` or `index` must be provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    /// The index of the tab to switch to (0-based).
    /// Exactly one of `handle` or `index` must be provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
}

// Custom deserialization to enforce that exactly one of `handle` or `index` is provided.
impl<'de> serde::Deserialize<'de> for SwitchTabParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawSwitchTabParams {
            handle: Option<String>,
            index: Option<usize>,
        }

        let raw = RawSwitchTabParams::deserialize(deserializer)?;

        match (&raw.handle, &raw.index) {
            (Some(_), Some(_)) => Err(serde::de::Error::custom(
                "Provide exactly one of 'handle' or 'index', not both",
            )),
            (None, None) => Err(serde::de::Error::custom(
                "Provide either 'handle' or 'index'",
            )),
            _ => Ok(SwitchTabParams {
                handle: raw.handle,
                index: raw.index,
            }),
        }
    }
}

/// Response type for tab list operation.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TabListResponse {
    /// List of all open tabs.
    pub tabs: Vec<TabInfo>,
    /// Whether the operation was successful.
    pub success: bool,
    /// Optional message describing the result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Response type for new tab operation.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NewTabResponse {
    /// Information about the newly created tab.
    pub tab: TabInfo,
    /// Whether the operation was successful.
    pub success: bool,
    /// Optional message describing the result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[tool_router]
impl BrowserMcpServer {
    /// Opens the web browser and returns the current state.
    #[tool(
        description = "Opens the web browser. Call this first before any other browser actions."
    )]
    async fn open_web_browser(&self) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::OPEN_WEB_BROWSER) {
            return disabled_tool_error(tool_names::OPEN_WEB_BROWSER);
        }
        self.touch();
        info!("Opening web browser");
        let result = self.browser.open().await;
        let tool_result = match &result {
            Ok(state) => env_state_to_result(state.clone(), Some("Browser opened successfully")),
            Err(e) => error_to_result(&format!("Failed to open browser: {}", e)),
        };
        self.operation_complete();

        // Start idle monitor after operation is complete (only if browser opened successfully)
        if result.is_ok() {
            self.start_idle_monitor().await;
        }

        tool_result
    }

    /// Clicks at a specific x, y coordinate on the webpage.
    #[tool(
        description = "Clicks at a specific x, y coordinate on the webpage. The coordinates are absolute values scaled to the screen dimensions."
    )]
    async fn click_at(
        &self,
        Parameters(params): Parameters<ClickAtParams>,
    ) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::CLICK_AT) {
            return disabled_tool_error(tool_names::CLICK_AT);
        }
        self.touch();
        info!("Clicking at ({}, {})", params.x, params.y);
        let result = match self.browser.click_at(params.x, params.y).await {
            Ok(state) => env_state_to_result(
                state,
                Some(&format!("Clicked at ({}, {})", params.x, params.y)),
            ),
            Err(e) => error_to_result(&format!("Failed to click: {}", e)),
        };
        self.operation_complete();
        result
    }

    /// Hovers at a specific x, y coordinate on the webpage.
    #[tool(
        description = "Hovers at a specific x, y coordinate on the webpage. May be used to explore sub-menus that appear on hover."
    )]
    async fn hover_at(
        &self,
        Parameters(params): Parameters<HoverAtParams>,
    ) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::HOVER_AT) {
            return disabled_tool_error(tool_names::HOVER_AT);
        }
        self.touch();
        info!("Hovering at ({}, {})", params.x, params.y);
        let result = match self.browser.hover_at(params.x, params.y).await {
            Ok(state) => env_state_to_result(
                state,
                Some(&format!("Hovered at ({}, {})", params.x, params.y)),
            ),
            Err(e) => error_to_result(&format!("Failed to hover: {}", e)),
        };
        self.operation_complete();
        result
    }

    /// Types text at a specific x, y coordinate.
    #[tool(
        description = "Types text at a specific x, y coordinate. The system can optionally press ENTER after typing and clear existing content before typing."
    )]
    async fn type_text_at(
        &self,
        Parameters(params): Parameters<TypeTextAtParams>,
    ) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::TYPE_TEXT_AT) {
            return disabled_tool_error(tool_names::TYPE_TEXT_AT);
        }
        self.touch();
        info!("Typing at ({}, {}): {}", params.x, params.y, params.text);
        let result = match self
            .browser
            .type_text_at(
                params.x,
                params.y,
                &params.text,
                params.press_enter,
                params.clear_before_typing,
            )
            .await
        {
            Ok(state) => env_state_to_result(
                state,
                Some(&format!(
                    "Typed '{}' at ({}, {})",
                    params.text, params.x, params.y
                )),
            ),
            Err(e) => error_to_result(&format!("Failed to type: {}", e)),
        };
        self.operation_complete();
        result
    }

    /// Scrolls the entire webpage in the specified direction.
    #[tool(
        description = "Scrolls the entire webpage 'up', 'down', 'left' or 'right' based on direction."
    )]
    async fn scroll_document(
        &self,
        Parameters(params): Parameters<ScrollDocumentParams>,
    ) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::SCROLL_DOCUMENT) {
            return disabled_tool_error(tool_names::SCROLL_DOCUMENT);
        }
        self.touch();
        info!("Scrolling document: {}", params.direction);
        let result = match self.browser.scroll_document(&params.direction).await {
            Ok(state) => env_state_to_result(
                state,
                Some(&format!("Scrolled document {}", params.direction)),
            ),
            Err(e) => error_to_result(&format!("Failed to scroll: {}", e)),
        };
        self.operation_complete();
        result
    }

    /// Scrolls at a specific coordinate in the specified direction.
    #[tool(
        description = "Scrolls up, down, right, or left at a x, y coordinate by magnitude pixels."
    )]
    async fn scroll_at(
        &self,
        Parameters(params): Parameters<ScrollAtParams>,
    ) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::SCROLL_AT) {
            return disabled_tool_error(tool_names::SCROLL_AT);
        }
        self.touch();
        info!(
            "Scrolling at ({}, {}) direction: {} magnitude: {}",
            params.x, params.y, params.direction, params.magnitude
        );
        let result = match self
            .browser
            .scroll_at(params.x, params.y, &params.direction, params.magnitude)
            .await
        {
            Ok(state) => env_state_to_result(
                state,
                Some(&format!(
                    "Scrolled {} at ({}, {}) by {} pixels",
                    params.direction, params.x, params.y, params.magnitude
                )),
            ),
            Err(e) => error_to_result(&format!("Failed to scroll: {}", e)),
        };
        self.operation_complete();
        result
    }

    /// Waits for 5 seconds to allow unfinished webpage processes to complete.
    #[tool(description = "Waits for 5 seconds to allow unfinished webpage processes to complete.")]
    async fn wait_5_seconds(&self) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::WAIT_5_SECONDS) {
            return disabled_tool_error(tool_names::WAIT_5_SECONDS);
        }
        self.touch();
        info!("Waiting 5 seconds");
        let result = match self.browser.wait_5_seconds().await {
            Ok(state) => env_state_to_result(state, Some("Waited 5 seconds")),
            Err(e) => error_to_result(&format!("Failed to wait: {}", e)),
        };
        self.operation_complete();
        result
    }

    /// Navigates back to the previous webpage in the browser history.
    #[tool(description = "Navigates back to the previous webpage in the browser history.")]
    async fn go_back(&self) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::GO_BACK) {
            return disabled_tool_error(tool_names::GO_BACK);
        }
        self.touch();
        info!("Going back");
        let result = match self.browser.go_back().await {
            Ok(state) => env_state_to_result(state, Some("Navigated back")),
            Err(e) => error_to_result(&format!("Failed to go back: {}", e)),
        };
        self.operation_complete();
        result
    }

    /// Navigates forward to the next webpage in the browser history.
    #[tool(description = "Navigates forward to the next webpage in the browser history.")]
    async fn go_forward(&self) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::GO_FORWARD) {
            return disabled_tool_error(tool_names::GO_FORWARD);
        }
        self.touch();
        info!("Going forward");
        let result = match self.browser.go_forward().await {
            Ok(state) => env_state_to_result(state, Some("Navigated forward")),
            Err(e) => error_to_result(&format!("Failed to go forward: {}", e)),
        };
        self.operation_complete();
        result
    }

    /// Directly jumps to a search engine home page.
    #[tool(
        description = "Directly jumps to a search engine home page. Used when you need to start with a search."
    )]
    async fn search(&self) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::SEARCH) {
            return disabled_tool_error(tool_names::SEARCH);
        }
        self.touch();
        info!("Navigating to search engine");
        let result = match self.browser.search().await {
            Ok(state) => env_state_to_result(state, Some("Navigated to search engine")),
            Err(e) => error_to_result(&format!("Failed to navigate to search: {}", e)),
        };
        self.operation_complete();
        result
    }

    /// Navigates directly to a specified URL.
    #[tool(
        description = "Navigates directly to a specified URL. URLs without a protocol will be prefixed with 'https://'."
    )]
    async fn navigate(
        &self,
        Parameters(params): Parameters<NavigateParams>,
    ) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::NAVIGATE) {
            return disabled_tool_error(tool_names::NAVIGATE);
        }
        self.touch();
        info!("Navigating to: {}", params.url);
        let result = match self.browser.navigate(&params.url).await {
            Ok(state) => env_state_to_result(state, Some(&format!("Navigated to {}", params.url))),
            Err(e) => error_to_result(&format!("Failed to navigate: {}", e)),
        };
        self.operation_complete();
        result
    }

    /// Presses keyboard keys and combinations.
    #[tool(
        description = "Presses keyboard keys and combinations, such as ['Control', 'c'] or ['Enter']. Supports modifiers like Control, Shift, Alt, Meta/Command."
    )]
    async fn key_combination(
        &self,
        Parameters(params): Parameters<KeyCombinationParams>,
    ) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::KEY_COMBINATION) {
            return disabled_tool_error(tool_names::KEY_COMBINATION);
        }
        self.touch();
        info!("Pressing key combination: {:?}", params.keys);
        let result = match self.browser.key_combination(params.keys.clone()).await {
            Ok(state) => {
                env_state_to_result(state, Some(&format!("Pressed keys: {:?}", params.keys)))
            }
            Err(e) => error_to_result(&format!("Failed to press keys: {}", e)),
        };
        self.operation_complete();
        result
    }

    /// Drag and drop an element from one position to another.
    #[tool(
        description = "Drag and drop an element from a x, y coordinate to a destination_x, destination_y coordinate."
    )]
    async fn drag_and_drop(
        &self,
        Parameters(params): Parameters<DragAndDropParams>,
    ) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::DRAG_AND_DROP) {
            return disabled_tool_error(tool_names::DRAG_AND_DROP);
        }
        self.touch();
        info!(
            "Drag and drop from ({}, {}) to ({}, {})",
            params.x, params.y, params.destination_x, params.destination_y
        );
        let result = match self
            .browser
            .drag_and_drop(
                params.x,
                params.y,
                params.destination_x,
                params.destination_y,
            )
            .await
        {
            Ok(state) => env_state_to_result(
                state,
                Some(&format!(
                    "Dragged from ({}, {}) to ({}, {})",
                    params.x, params.y, params.destination_x, params.destination_y
                )),
            ),
            Err(e) => error_to_result(&format!("Failed to drag and drop: {}", e)),
        };
        self.operation_complete();
        result
    }

    /// Returns the current state of the webpage.
    #[tool(
        description = "Returns the current state of the webpage including a screenshot and the current URL."
    )]
    async fn current_state(&self) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::CURRENT_STATE) {
            return disabled_tool_error(tool_names::CURRENT_STATE);
        }
        self.touch();
        info!("Getting current state");
        let result = match self.browser.current_state().await {
            Ok(state) => env_state_to_result(state, Some("Current state retrieved")),
            Err(e) => error_to_result(&format!("Failed to get current state: {}", e)),
        };
        self.operation_complete();
        result
    }

    // ========== Tab Management Tools ==========

    /// Creates a new browser tab.
    #[tool(
        description = "Creates a new browser tab. Optionally navigates to a URL in the new tab. Returns information about the new tab and a screenshot."
    )]
    async fn new_tab(
        &self,
        Parameters(params): Parameters<NewTabParams>,
    ) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::NEW_TAB) {
            return disabled_tool_error(tool_names::NEW_TAB);
        }
        self.touch();
        info!("Creating new tab with URL: {:?}", params.url);
        let result = match self.browser.new_tab(params.url.as_deref()).await {
            Ok((tab_info, state)) => {
                let response = NewTabResponse {
                    tab: tab_info,
                    success: true,
                    message: Some("New tab created successfully".to_string()),
                };
                let text = serde_json::to_string_pretty(&response)
                    .unwrap_or_else(|_| r#"{"success":true}"#.to_string());
                let text_content = Content::text(text);
                let image_content = Content::image(state.screenshot, "image/png");
                Ok(CallToolResult::success(vec![text_content, image_content]))
            }
            Err(e) => error_to_result(&format!("Failed to create new tab: {}", e)),
        };
        self.operation_complete();
        result
    }

    /// Closes a browser tab.
    #[tool(description = "Closes a browser tab. If no handle is provided, closes the current tab.")]
    async fn close_tab(
        &self,
        Parameters(params): Parameters<CloseTabParams>,
    ) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::CLOSE_TAB) {
            return disabled_tool_error(tool_names::CLOSE_TAB);
        }
        self.touch();
        info!("Closing tab: {:?}", params.handle);
        let result = match self.browser.close_tab(params.handle.as_deref()).await {
            Ok(state) => env_state_to_result(state, Some("Tab closed successfully")),
            Err(e) => error_to_result(&format!("Failed to close tab: {}", e)),
        };
        self.operation_complete();
        result
    }

    /// Switches to a different browser tab.
    #[tool(
        description = "Switches to a different browser tab by handle or index. Provide exactly one of 'handle' (window handle string) or 'index' (0-based tab index)."
    )]
    async fn switch_tab(
        &self,
        Parameters(params): Parameters<SwitchTabParams>,
    ) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::SWITCH_TAB) {
            return disabled_tool_error(tool_names::SWITCH_TAB);
        }
        self.touch();
        info!(
            "Switching to tab: handle={:?}, index={:?}",
            params.handle, params.index
        );
        let result = match self
            .browser
            .switch_tab(params.handle.as_deref(), params.index)
            .await
        {
            Ok(state) => env_state_to_result(state, Some("Switched to tab")),
            Err(e) => error_to_result(&format!("Failed to switch tab: {}", e)),
        };
        self.operation_complete();
        result
    }

    /// Lists all open browser tabs.
    #[tool(
        description = "Lists all open browser tabs with their handles, URLs, titles, and active status. Also returns a screenshot of the current tab."
    )]
    async fn list_tabs(&self) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::LIST_TABS) {
            return disabled_tool_error(tool_names::LIST_TABS);
        }
        self.touch();
        info!("Listing all tabs");
        let result = match self.browser.list_tabs().await {
            Ok((tabs, state)) => {
                let response = TabListResponse {
                    tabs,
                    success: true,
                    message: Some("Tabs listed successfully".to_string()),
                };
                let text = serde_json::to_string_pretty(&response)
                    .unwrap_or_else(|_| r#"{"success":true,"tabs":[]}"#.to_string());
                let text_content = Content::text(text);
                let image_content = Content::image(state.screenshot, "image/png");
                Ok(CallToolResult::success(vec![text_content, image_content]))
            }
            Err(e) => error_to_result(&format!("Failed to list tabs: {}", e)),
        };
        self.operation_complete();
        result
    }
}

#[tool_handler]
impl ServerHandler for BrowserMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "MCP server for browser control using Gemini computer use predefined tools. \
                Call 'open_web_browser' first to start the browser, then use other tools to interact with web pages."
                    .to_string(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "mcp-computer-use".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: None,
                icons: None,
                website_url: None,
            },
            ..Default::default()
        }
    }
}

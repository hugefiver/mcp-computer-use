//! MCP Tools implementation for browser control.
//!
//! This module defines all the MCP tools that expose browser control capabilities.

use crate::browser::{BrowserController, EnvState, TabInfo};
use crate::config::{tool_names, Config};
use rmcp::{
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::{
        CallToolResult, Content, ErrorData as McpError, Implementation, ServerCapabilities,
        ServerInfo,
    },
    schemars, tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::sync::Arc;
use tracing::info;

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
    browser: Arc<BrowserController>,
    config: Config,
    tool_router: ToolRouter<Self>,
}

impl BrowserMcpServer {
    /// Create a new MCP server with the given configuration.
    pub fn new(config: Config) -> Self {
        let browser = Arc::new(BrowserController::new(config.clone()));
        Self {
            browser,
            config,
            tool_router: Self::tool_router(),
        }
    }

    /// Get a reference to the browser controller.
    #[allow(dead_code)]
    pub fn browser(&self) -> &Arc<BrowserController> {
        &self.browser
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SwitchTabParams {
    /// The window handle of the tab to switch to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    /// The index of the tab to switch to (0-based).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
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
        info!("Opening web browser");
        match self.browser.open().await {
            Ok(state) => env_state_to_result(state, Some("Browser opened successfully")),
            Err(e) => error_to_result(&format!("Failed to open browser: {}", e)),
        }
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
        info!("Clicking at ({}, {})", params.x, params.y);
        match self.browser.click_at(params.x, params.y).await {
            Ok(state) => env_state_to_result(
                state,
                Some(&format!("Clicked at ({}, {})", params.x, params.y)),
            ),
            Err(e) => error_to_result(&format!("Failed to click: {}", e)),
        }
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
        info!("Hovering at ({}, {})", params.x, params.y);
        match self.browser.hover_at(params.x, params.y).await {
            Ok(state) => env_state_to_result(
                state,
                Some(&format!("Hovered at ({}, {})", params.x, params.y)),
            ),
            Err(e) => error_to_result(&format!("Failed to hover: {}", e)),
        }
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
        info!("Typing at ({}, {}): {}", params.x, params.y, params.text);
        match self
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
        }
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
        info!("Scrolling document: {}", params.direction);
        match self.browser.scroll_document(&params.direction).await {
            Ok(state) => env_state_to_result(
                state,
                Some(&format!("Scrolled document {}", params.direction)),
            ),
            Err(e) => error_to_result(&format!("Failed to scroll: {}", e)),
        }
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
        info!(
            "Scrolling at ({}, {}) direction: {} magnitude: {}",
            params.x, params.y, params.direction, params.magnitude
        );
        match self
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
        }
    }

    /// Waits for 5 seconds to allow unfinished webpage processes to complete.
    #[tool(description = "Waits for 5 seconds to allow unfinished webpage processes to complete.")]
    async fn wait_5_seconds(&self) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::WAIT_5_SECONDS) {
            return disabled_tool_error(tool_names::WAIT_5_SECONDS);
        }
        info!("Waiting 5 seconds");
        match self.browser.wait_5_seconds().await {
            Ok(state) => env_state_to_result(state, Some("Waited 5 seconds")),
            Err(e) => error_to_result(&format!("Failed to wait: {}", e)),
        }
    }

    /// Navigates back to the previous webpage in the browser history.
    #[tool(description = "Navigates back to the previous webpage in the browser history.")]
    async fn go_back(&self) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::GO_BACK) {
            return disabled_tool_error(tool_names::GO_BACK);
        }
        info!("Going back");
        match self.browser.go_back().await {
            Ok(state) => env_state_to_result(state, Some("Navigated back")),
            Err(e) => error_to_result(&format!("Failed to go back: {}", e)),
        }
    }

    /// Navigates forward to the next webpage in the browser history.
    #[tool(description = "Navigates forward to the next webpage in the browser history.")]
    async fn go_forward(&self) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::GO_FORWARD) {
            return disabled_tool_error(tool_names::GO_FORWARD);
        }
        info!("Going forward");
        match self.browser.go_forward().await {
            Ok(state) => env_state_to_result(state, Some("Navigated forward")),
            Err(e) => error_to_result(&format!("Failed to go forward: {}", e)),
        }
    }

    /// Directly jumps to a search engine home page.
    #[tool(
        description = "Directly jumps to a search engine home page. Used when you need to start with a search."
    )]
    async fn search(&self) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::SEARCH) {
            return disabled_tool_error(tool_names::SEARCH);
        }
        info!("Navigating to search engine");
        match self.browser.search().await {
            Ok(state) => env_state_to_result(state, Some("Navigated to search engine")),
            Err(e) => error_to_result(&format!("Failed to navigate to search: {}", e)),
        }
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
        info!("Navigating to: {}", params.url);
        match self.browser.navigate(&params.url).await {
            Ok(state) => env_state_to_result(state, Some(&format!("Navigated to {}", params.url))),
            Err(e) => error_to_result(&format!("Failed to navigate: {}", e)),
        }
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
        info!("Pressing key combination: {:?}", params.keys);
        match self.browser.key_combination(params.keys.clone()).await {
            Ok(state) => {
                env_state_to_result(state, Some(&format!("Pressed keys: {:?}", params.keys)))
            }
            Err(e) => error_to_result(&format!("Failed to press keys: {}", e)),
        }
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
        info!(
            "Drag and drop from ({}, {}) to ({}, {})",
            params.x, params.y, params.destination_x, params.destination_y
        );
        match self
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
        }
    }

    /// Returns the current state of the webpage.
    #[tool(
        description = "Returns the current state of the webpage including a screenshot and the current URL."
    )]
    async fn current_state(&self) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::CURRENT_STATE) {
            return disabled_tool_error(tool_names::CURRENT_STATE);
        }
        info!("Getting current state");
        match self.browser.current_state().await {
            Ok(state) => env_state_to_result(state, Some("Current state retrieved")),
            Err(e) => error_to_result(&format!("Failed to get current state: {}", e)),
        }
    }

    // ========== Tab Management Tools ==========

    /// Creates a new browser tab.
    #[tool(
        description = "Creates a new browser tab. Optionally navigates to a URL in the new tab. Returns information about the new tab."
    )]
    async fn new_tab(
        &self,
        Parameters(params): Parameters<NewTabParams>,
    ) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::NEW_TAB) {
            return disabled_tool_error(tool_names::NEW_TAB);
        }
        info!("Creating new tab with URL: {:?}", params.url);
        match self.browser.new_tab(params.url.as_deref()).await {
            Ok(tab_info) => {
                let response = NewTabResponse {
                    tab: tab_info,
                    success: true,
                    message: Some("New tab created successfully".to_string()),
                };
                let text = serde_json::to_string_pretty(&response)
                    .unwrap_or_else(|_| r#"{"success":true}"#.to_string());
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => error_to_result(&format!("Failed to create new tab: {}", e)),
        }
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
        info!("Closing tab: {:?}", params.handle);
        match self.browser.close_tab(params.handle.as_deref()).await {
            Ok(state) => env_state_to_result(state, Some("Tab closed successfully")),
            Err(e) => error_to_result(&format!("Failed to close tab: {}", e)),
        }
    }

    /// Switches to a different browser tab.
    #[tool(
        description = "Switches to a different browser tab by handle or index. Provide either 'handle' (window handle string) or 'index' (0-based tab index)."
    )]
    async fn switch_tab(
        &self,
        Parameters(params): Parameters<SwitchTabParams>,
    ) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::SWITCH_TAB) {
            return disabled_tool_error(tool_names::SWITCH_TAB);
        }
        info!(
            "Switching to tab: handle={:?}, index={:?}",
            params.handle, params.index
        );
        match self
            .browser
            .switch_tab(params.handle.as_deref(), params.index)
            .await
        {
            Ok(state) => env_state_to_result(state, Some("Switched to tab")),
            Err(e) => error_to_result(&format!("Failed to switch tab: {}", e)),
        }
    }

    /// Lists all open browser tabs.
    #[tool(
        description = "Lists all open browser tabs with their handles, URLs, titles, and active status."
    )]
    async fn list_tabs(&self) -> Result<CallToolResult, McpError> {
        if self.config.is_tool_disabled(tool_names::LIST_TABS) {
            return disabled_tool_error(tool_names::LIST_TABS);
        }
        info!("Listing all tabs");
        match self.browser.list_tabs().await {
            Ok(tabs) => {
                let response = TabListResponse {
                    tabs,
                    success: true,
                    message: Some("Tabs listed successfully".to_string()),
                };
                let text = serde_json::to_string_pretty(&response)
                    .unwrap_or_else(|_| r#"{"success":true,"tabs":[]}"#.to_string());
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => error_to_result(&format!("Failed to list tabs: {}", e)),
        }
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
            },
            ..Default::default()
        }
    }
}

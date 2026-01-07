//! Browser controller module using thirtyfour.
//!
//! This module provides browser automation capabilities using WebDriver.

use crate::config::{BrowserType, Config, ConnectionMode};
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thirtyfour::common::capabilities::chromium::ChromiumLikeCapabilities;
use thirtyfour::extensions::cdp::ChromeDevTools;
use thirtyfour::prelude::*;
use thirtyfour::WindowHandle;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Delay in milliseconds to wait for page to settle after actions.
const PAGE_SETTLE_DELAY_MS: u64 = 500;

/// Delay in milliseconds after typing actions.
const TYPING_DELAY_MS: u64 = 100;

/// Maximum number of retries for transient failures.
const MAX_RETRIES: u32 = 3;

/// Delay between retries in milliseconds.
const RETRY_DELAY_MS: u64 = 200;

/// Maximum safe integer value for JavaScript (2^53 - 1).
/// Coordinates beyond this could lose precision in JavaScript.
const MAX_SAFE_JS_INTEGER: i64 = 9007199254740991;

/// Maximum allowed scroll magnitude in pixels.
const MAX_SCROLL_MAGNITUDE: i64 = 10000;

/// Minimum allowed scroll magnitude in pixels.
const MIN_SCROLL_MAGNITUDE: i64 = 0;

/// Default user agent for undetected mode (realistic Chrome user agent).
const UNDETECTED_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// Key mapping from user-friendly names to WebDriver key names.
fn get_key_mapping(key: &str) -> &str {
    match key.to_lowercase().as_str() {
        "backspace" => "\u{E003}",
        "tab" => "\u{E004}",
        "return" | "enter" => "\u{E007}",
        "shift" => "\u{E008}",
        "control" | "ctrl" => "\u{E009}",
        "alt" => "\u{E00A}",
        "escape" | "esc" => "\u{E00C}",
        "space" => "\u{E00D}",
        "pageup" => "\u{E00E}",
        "pagedown" => "\u{E00F}",
        "end" => "\u{E010}",
        "home" => "\u{E011}",
        "left" | "arrowleft" => "\u{E012}",
        "up" | "arrowup" => "\u{E013}",
        "right" | "arrowright" => "\u{E014}",
        "down" | "arrowdown" => "\u{E015}",
        "insert" => "\u{E016}",
        "delete" => "\u{E017}",
        "f1" => "\u{E031}",
        "f2" => "\u{E032}",
        "f3" => "\u{E033}",
        "f4" => "\u{E034}",
        "f5" => "\u{E035}",
        "f6" => "\u{E036}",
        "f7" => "\u{E037}",
        "f8" => "\u{E038}",
        "f9" => "\u{E039}",
        "f10" => "\u{E03A}",
        "f11" => "\u{E03B}",
        "f12" => "\u{E03C}",
        "command" | "meta" => "\u{E03D}",
        _ => key,
    }
}

/// Retry a fallible async operation with exponential backoff.
///
/// This helper is useful for operations that might fail transiently,
/// such as WebDriver commands during page transitions.
///
/// # Panics
/// Panics at compile time if MAX_RETRIES is 0.
async fn retry_async<F, Fut, T, E>(operation_name: &str, mut f: F) -> std::result::Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = std::result::Result<T, E>>,
    E: std::fmt::Display,
{
    // Compile-time check that MAX_RETRIES > 0
    const { assert!(MAX_RETRIES > 0, "MAX_RETRIES must be greater than 0") }

    let mut last_error = None;
    for attempt in 0..MAX_RETRIES {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if attempt < MAX_RETRIES - 1 {
                    let delay = RETRY_DELAY_MS * (1 << attempt);
                    debug!(
                        "{} failed (attempt {}/{}): {}, retrying in {}ms",
                        operation_name,
                        attempt + 1,
                        MAX_RETRIES,
                        e,
                        delay
                    );
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
                last_error = Some(e);
            }
        }
    }

    // Safe to unwrap since we assert MAX_RETRIES > 0 and the loop always sets last_error
    Err(last_error.expect("retry_async: loop should have set last_error"))
}

/// Wait for page to be ready (document.readyState === 'complete').
async fn wait_for_page_ready(driver: &WebDriver) -> Result<()> {
    let timeout = Duration::from_secs(10);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        match driver.execute("return document.readyState", vec![]).await {
            Ok(result) => {
                let ready_state = result
                    .json()
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "loading".to_string());

                if ready_state == "complete" {
                    return Ok(());
                }
            }
            Err(e) => {
                debug!("Error checking page ready state: {}", e);
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Timeout is not an error - page might be slow loading
    debug!("Page load timeout reached, continuing anyway");
    Ok(())
}

/// Environment state returned by browser actions.
#[derive(Debug, Clone)]
pub struct EnvState {
    /// Screenshot in PNG format, base64 encoded.
    pub screenshot: String,
    /// Current URL of the page.
    pub url: String,
}

/// Information about a browser tab.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TabInfo {
    /// The window handle identifier.
    pub handle: String,
    /// The URL of the tab.
    pub url: String,
    /// The title of the tab.
    pub title: String,
    /// Whether this tab is currently active.
    pub active: bool,
    /// Navigation error if URL navigation failed (only set on new_tab).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub navigation_error: Option<String>,
}

/// Validate coordinates are within reasonable screen bounds and safe for JavaScript.
///
/// Coordinates are validated to ensure:
/// 1. They are not negative
/// 2. They are not too far outside screen bounds (allows some tolerance for edge cases)
/// 3. They are within JavaScript's safe integer range to prevent precision loss
fn validate_coordinates(x: i64, y: i64, width: u32, height: u32) -> Result<()> {
    if x < 0 || y < 0 {
        return Err(anyhow::anyhow!(
            "Coordinates cannot be negative: ({}, {})",
            x,
            y
        ));
    }
    if x > MAX_SAFE_JS_INTEGER || y > MAX_SAFE_JS_INTEGER {
        return Err(anyhow::anyhow!(
            "Coordinates ({}, {}) exceed JavaScript safe integer limit",
            x,
            y
        ));
    }
    if x as u32 > width * 2 || y as u32 > height * 2 {
        return Err(anyhow::anyhow!(
            "Coordinates ({}, {}) are too far outside screen bounds ({}x{})",
            x,
            y,
            width,
            height
        ));
    }
    Ok(())
}

/// Validate scroll magnitude is within reasonable bounds.
fn validate_magnitude(magnitude: i64) -> Result<()> {
    if magnitude < MIN_SCROLL_MAGNITUDE {
        return Err(anyhow::anyhow!(
            "Scroll magnitude {} is below minimum allowed value {}",
            magnitude,
            MIN_SCROLL_MAGNITUDE
        ));
    }
    if magnitude > MAX_SCROLL_MAGNITUDE {
        return Err(anyhow::anyhow!(
            "Scroll magnitude {} exceeds maximum allowed value {}",
            magnitude,
            MAX_SCROLL_MAGNITUDE
        ));
    }
    Ok(())
}

/// Valid key names for keyboard input (case-insensitive).
static VALID_KEY_NAMES: &[&str] = &[
    "backspace",
    "tab",
    "return",
    "enter",
    "shift",
    "control",
    "ctrl",
    "alt",
    "escape",
    "esc",
    "space",
    "pageup",
    "pagedown",
    "end",
    "home",
    "left",
    "arrowleft",
    "up",
    "arrowup",
    "right",
    "arrowright",
    "down",
    "arrowdown",
    "insert",
    "delete",
    "command",
    "meta",
    "f1",
    "f2",
    "f3",
    "f4",
    "f5",
    "f6",
    "f7",
    "f8",
    "f9",
    "f10",
    "f11",
    "f12",
];

/// Safe single-character keys that can be used in JavaScript strings.
/// Excludes backticks, quotes, and backslashes to prevent JavaScript injection.
static SAFE_SINGLE_CHAR_KEYS: &[char] = &[
    // Letters and numbers are handled separately via is_ascii_alphanumeric()
    // Safe punctuation that won't break JavaScript strings
    '!', '@', '#', '$', '%', '^', '&', '*', '(', ')', '-', '_', '=', '+', '[', ']', '{', '}', ';',
    ':', ',', '.', '<', '>', '/', '?', '|', '~',
];

/// Validate that a key name is safe for use in JavaScript.
/// Only allows alphanumeric characters, common key names, and safe single characters.
fn validate_key_name(key: &str) -> Result<()> {
    let lower = key.to_lowercase();

    // Check if it's a known key name
    if VALID_KEY_NAMES.contains(&lower.as_str()) {
        return Ok(());
    }

    // Allow single safe characters (letters, numbers, and safe punctuation)
    if key.len() == 1 {
        let c = key.chars().next().unwrap();
        if c.is_ascii_alphanumeric() || SAFE_SINGLE_CHAR_KEYS.contains(&c) {
            return Ok(());
        }
    }

    Err(anyhow::anyhow!(
        "Invalid key name '{}'. Only known key names and single printable characters are allowed.",
        key
    ))
}

/// Browser controller that wraps WebDriver operations.
pub struct BrowserController {
    driver: Arc<Mutex<Option<WebDriver>>>,
    config: Config,
    /// Tracks whether the browser was opened (and thus needs cleanup)
    was_opened: AtomicBool,
    /// Tracks whether close() was called
    was_closed: AtomicBool,
}

impl BrowserController {
    /// Create a new browser controller with the given configuration.
    pub fn new(config: Config) -> Self {
        Self {
            driver: Arc::new(Mutex::new(None)),
            config,
            was_opened: AtomicBool::new(false),
            was_closed: AtomicBool::new(false),
        }
    }

    /// Initialize and open the browser.
    pub async fn open(&self) -> Result<EnvState> {
        let mut driver_guard = self.driver.lock().await;

        if driver_guard.is_some() {
            // Browser already open, just return current state
            drop(driver_guard);
            return self.current_state().await;
        }

        info!("Opening {:?} browser...", self.config.browser_type);

        // Get the WebDriver URL
        let webdriver_url = self.config.effective_webdriver_url();

        // Create driver based on browser type
        let driver = match self.config.browser_type {
            BrowserType::Chrome => self.create_chrome_driver(&webdriver_url).await?,
            BrowserType::Edge => self.create_edge_driver(&webdriver_url).await?,
            BrowserType::Firefox => self.create_firefox_driver(&webdriver_url).await?,
            BrowserType::Safari => self.create_safari_driver(&webdriver_url).await?,
        };

        // Set window size
        if self.config.connection_mode != ConnectionMode::Cdp {
            driver
                .set_window_rect(0, 0, self.config.screen_width, self.config.screen_height)
                .await?;

            // Navigate to initial URL
            driver.goto(&self.config.initial_url).await?;
        }

        *driver_guard = Some(driver);
        self.was_opened.store(true, Ordering::SeqCst);
        drop(driver_guard);

        info!("Browser opened successfully");
        self.current_state().await
    }

    /// Create a Chrome WebDriver.
    async fn create_chrome_driver(&self, webdriver_url: &str) -> Result<WebDriver> {
        let mut caps = DesiredCapabilities::chrome();

        // In CDP mode, connect to existing browser via debuggerAddress
        if self.config.connection_mode == ConnectionMode::Cdp {
            let cdp_port = self.config.effective_cdp_port();
            let debugger_address = format!("127.0.0.1:{}", cdp_port);
            info!(
                "CDP mode: connecting to existing browser at {}",
                debugger_address
            );
            caps.add_experimental_option("debuggerAddress", debugger_address)?;
        } else {
            // WebDriver mode: configure browser options
            self.configure_chromium_caps(&mut caps)?;
        }

        let driver = WebDriver::new(webdriver_url, caps).await?;

        // Apply stealth scripts for Chrome if undetected mode is enabled
        if self.config.undetected && self.config.connection_mode != ConnectionMode::Cdp {
            self.apply_chromium_stealth_scripts(&driver).await;
        }

        Ok(driver)
    }

    /// Create an Edge WebDriver.
    async fn create_edge_driver(&self, webdriver_url: &str) -> Result<WebDriver> {
        let mut caps = DesiredCapabilities::edge();

        // In CDP mode, connect to existing browser via debuggerAddress
        if self.config.connection_mode == ConnectionMode::Cdp {
            let cdp_port = self.config.effective_cdp_port();
            let debugger_address = format!("127.0.0.1:{}", cdp_port);
            info!(
                "CDP mode: connecting to existing Edge browser at {}",
                debugger_address
            );
            caps.add_experimental_option("debuggerAddress", debugger_address)?;
        } else {
            // WebDriver mode: configure browser options (Edge uses same args as Chrome)
            self.configure_chromium_caps(&mut caps)?;
        }

        let driver = WebDriver::new(webdriver_url, caps).await?;

        // Apply stealth scripts for Edge if undetected mode is enabled
        if self.config.undetected && self.config.connection_mode != ConnectionMode::Cdp {
            self.apply_chromium_stealth_scripts(&driver).await;
        }

        Ok(driver)
    }

    /// Create a Firefox WebDriver.
    async fn create_firefox_driver(&self, webdriver_url: &str) -> Result<WebDriver> {
        let mut caps = DesiredCapabilities::firefox();

        // Firefox headless mode
        if self.config.headless {
            caps.add_arg("--headless")?;
        }

        // Set window size via arguments for Firefox
        caps.add_arg(&format!("--width={}", self.config.screen_width))?;
        caps.add_arg(&format!("--height={}", self.config.screen_height))?;

        // Set binary path if specified
        if let Some(ref binary_path) = self.config.browser_binary_path {
            caps.set_firefox_binary(binary_path.to_string_lossy().as_ref())?;
        }

        let driver = WebDriver::new(webdriver_url, caps).await?;

        // Apply Firefox-specific stealth if undetected mode is enabled
        if self.config.undetected {
            let stealth_script = r#"
                Object.defineProperty(navigator, 'webdriver', {
                    get: () => undefined
                });
            "#;
            if let Err(e) = driver.execute(stealth_script, vec![]).await {
                warn!("Failed to apply Firefox stealth script: {}", e);
            }
        }

        Ok(driver)
    }

    /// Create a Safari WebDriver.
    async fn create_safari_driver(&self, webdriver_url: &str) -> Result<WebDriver> {
        let caps = DesiredCapabilities::safari();
        // Safari has limited customization options
        let driver = WebDriver::new(webdriver_url, caps).await?;
        Ok(driver)
    }

    /// Configure Chromium-based browser capabilities (Chrome/Edge).
    fn configure_chromium_caps<C: ChromiumLikeCapabilities>(&self, caps: &mut C) -> Result<()> {
        if self.config.headless {
            caps.add_arg("--headless=new")?;
        }
        caps.add_arg("--disable-extensions")?;
        caps.add_arg("--disable-plugins")?;
        caps.add_arg("--disable-dev-shm-usage")?;
        caps.add_arg("--disable-background-networking")?;
        caps.add_arg("--disable-default-apps")?;
        caps.add_arg("--disable-sync")?;
        caps.add_arg("--no-sandbox")?;
        caps.add_arg(&format!(
            "--window-size={},{}",
            self.config.screen_width, self.config.screen_height
        ))?;

        // Undetected mode settings (inspired by patchright/undetected-chromedriver)
        if self.config.undetected {
            info!("Enabling undetected mode");
            caps.add_exclude_switch("enable-automation")?;
            caps.add_experimental_option("useAutomationExtension", false)?;
            caps.add_arg("--disable-infobars")?;
            caps.add_arg("--disable-popup-blocking")?;
            caps.add_arg("--disable-notifications")?;
            caps.add_arg(&format!("--user-agent={}", UNDETECTED_USER_AGENT))?;
        }

        if let Some(ref binary_path) = self.config.browser_binary_path {
            caps.set_binary(binary_path.to_string_lossy().as_ref())?;
        }

        Ok(())
    }

    /// Apply stealth scripts for Chromium-based browsers.
    async fn apply_chromium_stealth_scripts(&self, driver: &WebDriver) {
        let stealth_script = r#"
            Object.defineProperty(navigator, 'webdriver', {
                get: () => undefined
            });
            
            // Override plugins property to resemble a real browser
            Object.defineProperty(navigator, 'plugins', {
                get: () => ([
                    {
                        name: 'Chrome PDF Plugin',
                        filename: 'internal-pdf-viewer',
                        description: 'Portable Document Format',
                        length: 1
                    },
                    {
                        name: 'Chrome PDF Viewer',
                        filename: 'mhjfbmdgcfjbbpaeojofohoefgiehjai',
                        description: '',
                        length: 1
                    }
                ])
            });
            
            // Override permissions query
            const originalQuery = window.navigator.permissions.query;
            window.navigator.permissions.query = (parameters) => (
                parameters.name === 'notifications' ?
                Promise.resolve({ state: Notification.permission }) :
                originalQuery(parameters)
            );
            
            // Overwrite the headless check
            Object.defineProperty(navigator, 'languages', {
                get: () => ['en-US', 'en']
            });
        "#;

        // Use CDP to add script that runs on every new document
        let dev_tools = ChromeDevTools::new(driver.handle.clone());
        let cdp_cmd = serde_json::json!({
            "source": stealth_script
        });
        if let Err(e) = dev_tools
            .execute_cdp_with_params("Page.addScriptToEvaluateOnNewDocument", cdp_cmd)
            .await
        {
            warn!(
                "Failed to add stealth script via CDP (undetected mode may not work fully): {}",
                e
            );
        }

        // Also execute immediately for the current page
        if let Err(e) = driver.execute(stealth_script, vec![]).await {
            warn!("Failed to execute stealth script: {}", e);
        }
    }

    /// Close the browser.
    #[allow(dead_code)]
    pub async fn close(&self) -> Result<()> {
        let mut driver_guard = self.driver.lock().await;
        if let Some(driver) = driver_guard.take() {
            driver.quit().await?;
            self.was_closed.store(true, Ordering::SeqCst);
            info!("Browser closed");
        }
        Ok(())
    }

    /// Get the current state (screenshot and URL).
    pub async fn current_state(&self) -> Result<EnvState> {
        let driver_guard = self.driver.lock().await;
        let driver = driver_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Browser not opened"))?;

        // Wait for page to be ready
        let _ = wait_for_page_ready(driver).await;

        // Additional settle time for dynamic content
        tokio::time::sleep(Duration::from_millis(PAGE_SETTLE_DELAY_MS)).await;

        // Use retry for screenshot in case of transient failures
        let screenshot_bytes =
            retry_async("screenshot", || async { driver.screenshot_as_png().await }).await?;
        let screenshot = BASE64.encode(&screenshot_bytes);
        let url = driver.current_url().await?.to_string();

        Ok(EnvState { screenshot, url })
    }

    /// Click at specific coordinates.
    pub async fn click_at(&self, x: i64, y: i64) -> Result<EnvState> {
        validate_coordinates(x, y, self.config.screen_width, self.config.screen_height)?;
        debug!("Clicking at ({}, {})", x, y);
        let driver_guard = self.driver.lock().await;
        let driver = driver_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Browser not opened"))?;

        // Try to find element at coordinates and click it with proper event dispatch
        // Note: x and y are i64, so format! only produces numeric values (no injection risk)
        let script = format!(
            r#"
            (function() {{
                var element = document.elementFromPoint({}, {});
                if (element) {{
                    // Scroll element into view if needed
                    element.scrollIntoView({{block: 'center', inline: 'center', behavior: 'instant'}});
                    
                    // Dispatch mousedown, mouseup, and click events for better compatibility
                    var rect = element.getBoundingClientRect();
                    var events = ['mousedown', 'mouseup', 'click'];
                    events.forEach(function(eventType) {{
                        var event = new MouseEvent(eventType, {{
                            view: window,
                            bubbles: true,
                            cancelable: true,
                            clientX: rect.left + rect.width / 2,
                            clientY: rect.top + rect.height / 2,
                            button: 0
                        }});
                        element.dispatchEvent(event);
                    }});
                    
                    // Also try direct click as fallback
                    if (typeof element.click === 'function') {{
                        element.click();
                    }}
                    return true;
                }}
                return false;
            }})();
            "#,
            x, y
        );

        let result = driver.execute(&script, vec![]).await?;
        let clicked = result.json().as_bool().unwrap_or(false);

        if !clicked {
            debug!(
                "No element found at ({}, {}), dispatching raw click event",
                x, y
            );

            // Fallback: dispatch raw mouse events at the given coordinates
            let raw_click_script = format!(
                r#"
                (function() {{
                    try {{
                        var events = ['mousedown', 'mouseup', 'click'];
                        var success = false;
                        events.forEach(function(eventType) {{
                            var event = new MouseEvent(eventType, {{
                                view: window,
                                bubbles: true,
                                cancelable: true,
                                clientX: {},
                                clientY: {},
                                button: 0
                            }});
                            success = document.dispatchEvent(event) || success;
                        }});
                        return success;
                    }} catch (e) {{
                        return false;
                    }}
                }})();
                "#,
                x, y
            );

            let raw_result = driver.execute(&raw_click_script, vec![]).await?;
            let raw_clicked = raw_result.json().as_bool().unwrap_or(false);

            if !raw_clicked {
                debug!(
                    "Raw click event at ({}, {}) may not have been handled by the page",
                    x, y
                );
            }
        }

        // Wait for potential navigation or page changes
        let _ = wait_for_page_ready(driver).await;

        drop(driver_guard);
        self.current_state().await
    }

    /// Hover at specific coordinates.
    pub async fn hover_at(&self, x: i64, y: i64) -> Result<EnvState> {
        validate_coordinates(x, y, self.config.screen_width, self.config.screen_height)?;
        debug!("Hovering at ({}, {})", x, y);
        let driver_guard = self.driver.lock().await;
        let driver = driver_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Browser not opened"))?;

        // Use JavaScript to simulate hover with full mouse event sequence
        let script = format!(
            r#"
            (function() {{
                var element = document.elementFromPoint({}, {});
                if (element) {{
                    // Dispatch mouseenter and mouseover events for proper hover behavior
                    var events = ['mouseenter', 'mouseover', 'mousemove'];
                    events.forEach(function(eventType) {{
                        var event = new MouseEvent(eventType, {{
                            view: window,
                            bubbles: true,
                            cancelable: true,
                            clientX: {},
                            clientY: {}
                        }});
                        element.dispatchEvent(event);
                    }});
                    return true;
                }}
                return false;
            }})();
            "#,
            x, y, x, y
        );
        driver.execute(&script, vec![]).await?;

        // Give time for hover menus/effects to appear
        tokio::time::sleep(Duration::from_millis(PAGE_SETTLE_DELAY_MS)).await;

        drop(driver_guard);
        self.current_state().await
    }

    /// Type text at specific coordinates.
    pub async fn type_text_at(
        &self,
        x: i64,
        y: i64,
        text: &str,
        press_enter: bool,
        clear_before_typing: bool,
    ) -> Result<EnvState> {
        validate_coordinates(x, y, self.config.screen_width, self.config.screen_height)?;
        debug!("Typing at ({}, {}): {}", x, y, text);
        let driver_guard = self.driver.lock().await;
        let driver = driver_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Browser not opened"))?;

        // Click at the position first
        // Note: x and y are i64, so format! only produces numeric values (no injection risk)
        let click_script = format!(
            r#"
            var element = document.elementFromPoint({}, {});
            if (element) {{
                element.click();
                element.focus();
            }}
            "#,
            x, y
        );
        driver.execute(&click_script, vec![]).await?;
        tokio::time::sleep(Duration::from_millis(TYPING_DELAY_MS)).await;

        // Find active element and interact with it
        let active_element = driver.active_element().await?;

        if clear_before_typing {
            // Select all and delete
            active_element.send_keys(Key::Control + "a").await?;
            active_element.send_keys(Key::Delete).await?;
        }

        // Type the text
        active_element.send_keys(text).await?;

        if press_enter {
            active_element.send_keys(Key::Enter).await?;
        }

        tokio::time::sleep(Duration::from_millis(PAGE_SETTLE_DELAY_MS)).await;

        drop(driver_guard);
        self.current_state().await
    }

    /// Scroll the entire document.
    pub async fn scroll_document(&self, direction: &str) -> Result<EnvState> {
        debug!("Scrolling document: {}", direction);
        let driver_guard = self.driver.lock().await;
        let driver = driver_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Browser not opened"))?;

        let script = match direction.to_lowercase().as_str() {
            "up" => "window.scrollBy(0, -window.innerHeight * 0.8);",
            "down" => "window.scrollBy(0, window.innerHeight * 0.8);",
            "left" => "window.scrollBy(-window.innerWidth * 0.5, 0);",
            "right" => "window.scrollBy(window.innerWidth * 0.5, 0);",
            _ => return Err(anyhow::anyhow!("Invalid scroll direction: {}", direction)),
        };

        driver.execute(script, vec![]).await?;

        drop(driver_guard);
        self.current_state().await
    }

    /// Scroll at specific coordinates.
    pub async fn scroll_at(
        &self,
        x: i64,
        y: i64,
        direction: &str,
        magnitude: i64,
    ) -> Result<EnvState> {
        validate_coordinates(x, y, self.config.screen_width, self.config.screen_height)?;
        validate_magnitude(magnitude)?;
        debug!(
            "Scrolling at ({}, {}) direction: {} magnitude: {}",
            x, y, direction, magnitude
        );
        let driver_guard = self.driver.lock().await;
        let driver = driver_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Browser not opened"))?;

        let (dx, dy) = match direction.to_lowercase().as_str() {
            "up" => (0, -magnitude),
            "down" => (0, magnitude),
            "left" => (-magnitude, 0),
            "right" => (magnitude, 0),
            _ => return Err(anyhow::anyhow!("Invalid scroll direction: {}", direction)),
        };

        let script = format!(
            r#"
            var element = document.elementFromPoint({}, {});
            if (element) {{
                element.scrollBy({}, {});
            }} else {{
                window.scrollBy({}, {});
            }}
            "#,
            x, y, dx, dy, dx, dy
        );

        driver.execute(&script, vec![]).await?;

        drop(driver_guard);
        self.current_state().await
    }

    /// Wait for 5 seconds.
    pub async fn wait_5_seconds(&self) -> Result<EnvState> {
        debug!("Waiting 5 seconds");
        tokio::time::sleep(Duration::from_secs(5)).await;
        self.current_state().await
    }

    /// Navigate back.
    pub async fn go_back(&self) -> Result<EnvState> {
        debug!("Going back");
        let driver_guard = self.driver.lock().await;
        let driver = driver_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Browser not opened"))?;

        driver.back().await?;
        tokio::time::sleep(Duration::from_millis(PAGE_SETTLE_DELAY_MS)).await;

        drop(driver_guard);
        self.current_state().await
    }

    /// Navigate forward.
    pub async fn go_forward(&self) -> Result<EnvState> {
        debug!("Going forward");
        let driver_guard = self.driver.lock().await;
        let driver = driver_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Browser not opened"))?;

        driver.forward().await?;
        tokio::time::sleep(Duration::from_millis(PAGE_SETTLE_DELAY_MS)).await;

        drop(driver_guard);
        self.current_state().await
    }

    /// Navigate to search engine.
    pub async fn search(&self) -> Result<EnvState> {
        debug!("Navigating to search engine");
        self.navigate(&self.config.search_engine_url.clone()).await
    }

    /// Navigate to a specific URL.
    pub async fn navigate(&self, url: &str) -> Result<EnvState> {
        debug!("Navigating to: {}", url);
        let driver_guard = self.driver.lock().await;
        let driver = driver_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Browser not opened"))?;

        let normalized_url = if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else {
            format!("https://{}", url)
        };

        driver.goto(&normalized_url).await?;

        // Wait for page to be fully loaded
        let _ = wait_for_page_ready(driver).await;

        drop(driver_guard);
        self.current_state().await
    }

    /// Press key combination.
    pub async fn key_combination(&self, keys: Vec<String>) -> Result<EnvState> {
        debug!("Pressing key combination: {:?}", keys);

        if keys.is_empty() {
            return Err(anyhow::anyhow!("No keys provided"));
        }

        // Validate all keys before proceeding
        for key in &keys {
            validate_key_name(key)?;
        }

        let driver_guard = self.driver.lock().await;
        let driver = driver_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Browser not opened"))?;

        // Build the key sequence using JavaScript
        let mut key_codes = Vec::new();
        for key in &keys {
            let mapped = get_key_mapping(key);
            key_codes.push(mapped.to_string());
        }

        // For simple key combinations, we'll use the active element
        let active_element = driver.active_element().await?;

        // Build combined key string for thirtyfour
        if keys.len() == 1 {
            active_element.send_keys(&key_codes[0]).await?;
        } else {
            // For multi-key combinations, execute via JavaScript
            let ctrl = keys
                .iter()
                .any(|k| k.to_lowercase() == "control" || k.to_lowercase() == "ctrl");
            let shift = keys.iter().any(|k| k.to_lowercase() == "shift");
            let alt = keys.iter().any(|k| k.to_lowercase() == "alt");
            let meta = keys
                .iter()
                .any(|k| k.to_lowercase() == "meta" || k.to_lowercase() == "command");

            // Find the main key (non-modifier)
            let main_key = keys.iter().find(|k| {
                let lower = k.to_lowercase();
                !["control", "ctrl", "shift", "alt", "meta", "command"].contains(&lower.as_str())
            });

            if let Some(key) = main_key {
                // Use JSON encoding for safe JavaScript string interpolation
                let escaped_key = serde_json::to_string(&key.to_lowercase())
                    .unwrap_or_else(|_| format!("\"{}\"", key.to_lowercase()));
                let script = format!(
                    r#"
                    var event = new KeyboardEvent('keydown', {{
                        key: {},
                        ctrlKey: {},
                        shiftKey: {},
                        altKey: {},
                        metaKey: {},
                        bubbles: true
                    }});
                    document.activeElement.dispatchEvent(event);
                    "#,
                    escaped_key, ctrl, shift, alt, meta
                );
                driver.execute(&script, vec![]).await?;
            }
        }

        drop(driver_guard);
        self.current_state().await
    }

    /// Drag and drop from one position to another.
    pub async fn drag_and_drop(
        &self,
        x: i64,
        y: i64,
        destination_x: i64,
        destination_y: i64,
    ) -> Result<EnvState> {
        validate_coordinates(x, y, self.config.screen_width, self.config.screen_height)?;
        validate_coordinates(
            destination_x,
            destination_y,
            self.config.screen_width,
            self.config.screen_height,
        )?;
        debug!(
            "Drag and drop from ({}, {}) to ({}, {})",
            x, y, destination_x, destination_y
        );
        let driver_guard = self.driver.lock().await;
        let driver = driver_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Browser not opened"))?;

        // Use JavaScript to simulate drag and drop
        // Note: All coordinates are i64, so format! only produces numeric values (no injection risk)
        let script = format!(
            r#"
            function simulateDragDrop(startX, startY, endX, endY) {{
                var startElement = document.elementFromPoint(startX, startY);
                if (!startElement) return;

                var dataTransfer = new DataTransfer();

                var dragStartEvent = new DragEvent('dragstart', {{
                    bubbles: true,
                    cancelable: true,
                    dataTransfer: dataTransfer,
                    clientX: startX,
                    clientY: startY
                }});
                startElement.dispatchEvent(dragStartEvent);

                var dragEvent = new DragEvent('drag', {{
                    bubbles: true,
                    cancelable: true,
                    dataTransfer: dataTransfer,
                    clientX: endX,
                    clientY: endY
                }});
                startElement.dispatchEvent(dragEvent);

                var endElement = document.elementFromPoint(endX, endY);
                if (endElement) {{
                    var dropEvent = new DragEvent('drop', {{
                        bubbles: true,
                        cancelable: true,
                        dataTransfer: dataTransfer,
                        clientX: endX,
                        clientY: endY
                    }});
                    endElement.dispatchEvent(dropEvent);
                }}

                var dragEndEvent = new DragEvent('dragend', {{
                    bubbles: true,
                    cancelable: true,
                    dataTransfer: dataTransfer,
                    clientX: endX,
                    clientY: endY
                }});
                startElement.dispatchEvent(dragEndEvent);
            }}
            simulateDragDrop({}, {}, {}, {});
            "#,
            x, y, destination_x, destination_y
        );

        driver.execute(&script, vec![]).await?;

        drop(driver_guard);
        self.current_state().await
    }

    // ========== Tab Management Methods ==========

    /// Create a new browser tab and optionally navigate to a URL.
    /// Create a new browser tab and optionally navigate to a URL.
    /// Returns both tab info and the current environment state.
    pub async fn new_tab(&self, url: Option<&str>) -> Result<(TabInfo, EnvState)> {
        debug!("Creating new tab");
        let driver_guard = self.driver.lock().await;
        let driver = driver_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Browser not opened"))?;

        // Create new tab
        let new_handle = driver.new_tab().await?;

        // Switch to the new tab
        driver.switch_to_window(new_handle.clone()).await?;

        // Navigate to URL if provided, handling failures gracefully
        let mut navigation_error: Option<String> = None;
        if let Some(url) = url {
            let normalized_url = if url.starts_with("http://") || url.starts_with("https://") {
                url.to_string()
            } else {
                format!("https://{}", url)
            };
            if let Err(e) = driver.goto(&normalized_url).await {
                // Log the error but don't fail - tab is still created
                warn!("Navigation failed in new tab: {}. Tab remains open.", e);
                navigation_error = Some(format!("Navigation failed: {}", e));
            }
        }

        tokio::time::sleep(Duration::from_millis(PAGE_SETTLE_DELAY_MS)).await;

        let current_url = driver.current_url().await?.to_string();
        let title = driver.title().await.unwrap_or_default();

        let tab_info = TabInfo {
            handle: new_handle.to_string(),
            url: current_url.clone(),
            title,
            active: true,
            navigation_error,
        };

        // Get screenshot for the state
        let screenshot_bytes = driver.screenshot_as_png().await?;
        let screenshot = BASE64.encode(&screenshot_bytes);

        let state = EnvState {
            screenshot,
            url: current_url,
        };

        Ok((tab_info, state))
    }

    /// Close a browser tab by handle.
    pub async fn close_tab(&self, handle: Option<&str>) -> Result<EnvState> {
        debug!("Closing tab: {:?}", handle);
        let driver_guard = self.driver.lock().await;
        let driver = driver_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Browser not opened"))?;

        if let Some(handle) = handle {
            // Switch to the specified tab first
            let window_handle = WindowHandle::from(handle.to_string());
            driver.switch_to_window(window_handle).await?;
        }

        // Determine which window to switch to (if any) before closing.
        let windows = driver.windows().await?;
        let current = driver.window().await.ok();
        let next_window = windows.into_iter().find(|w| Some(w) != current.as_ref());

        // Close current window
        driver.close_window().await?;

        // If there is another window, switch to it
        if let Some(other) = next_window {
            driver.switch_to_window(other).await?;
        }

        drop(driver_guard);
        self.current_state().await
    }

    /// Switch to a tab by handle or index.
    /// Exactly one of handle or index must be provided.
    pub async fn switch_tab(&self, handle: Option<&str>, index: Option<usize>) -> Result<EnvState> {
        debug!("Switching to tab: handle={:?}, index={:?}", handle, index);

        // Validate that exactly one of handle or index is provided
        match (&handle, &index) {
            (Some(_), Some(_)) => {
                return Err(anyhow::anyhow!(
                    "Provide exactly one of 'handle' or 'index', not both"
                ));
            }
            (None, None) => {
                return Err(anyhow::anyhow!("Either handle or index must be provided"));
            }
            _ => {}
        }

        let driver_guard = self.driver.lock().await;
        let driver = driver_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Browser not opened"))?;

        if let Some(handle) = handle {
            let window_handle = WindowHandle::from(handle.to_string());
            driver.switch_to_window(window_handle).await?;
        } else if let Some(index) = index {
            let windows = driver.windows().await?;
            let window = windows
                .into_iter()
                .nth(index)
                .ok_or_else(|| anyhow::anyhow!("Tab index {} out of range", index))?;
            driver.switch_to_window(window).await?;
        }

        tokio::time::sleep(Duration::from_millis(PAGE_SETTLE_DELAY_MS)).await;

        drop(driver_guard);
        self.current_state().await
    }

    /// List all open tabs and return current state.
    pub async fn list_tabs(&self) -> Result<(Vec<TabInfo>, EnvState)> {
        debug!("Listing all tabs");
        let driver_guard = self.driver.lock().await;
        let driver = driver_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Browser not opened"))?;

        let current_handle = driver.window().await?;
        let windows = driver.windows().await?;
        let mut tabs = Vec::new();

        // Perform the tab enumeration, capturing any error.
        let result: Result<Vec<TabInfo>> = async {
            for window in windows {
                let is_active = window == current_handle;
                driver.switch_to_window(window.clone()).await?;

                let url = driver.current_url().await?.to_string();
                let title = driver.title().await.unwrap_or_default();

                tabs.push(TabInfo {
                    handle: window.to_string(),
                    url,
                    title,
                    active: is_active,
                    navigation_error: None,
                });
            }

            Ok(tabs)
        }
        .await;

        // Always attempt to switch back to the original tab,
        // even if an error occurred during enumeration.
        if let Err(e) = driver.switch_to_window(current_handle).await {
            warn!("Failed to switch back to original tab: {:?}", e);
        }

        let tabs = result?;

        // Get current state (screenshot and URL)
        let screenshot_bytes = driver.screenshot_as_png().await?;
        let screenshot = BASE64.encode(&screenshot_bytes);
        let url = driver.current_url().await?.to_string();

        let state = EnvState { screenshot, url };

        Ok((tabs, state))
    }

    /// Get the screen size.
    #[allow(dead_code)]
    pub fn screen_size(&self) -> (u32, u32) {
        (self.config.screen_width, self.config.screen_height)
    }
}

impl Drop for BrowserController {
    fn drop(&mut self) {
        // Use atomic flags to reliably detect if cleanup is needed
        // This is more reliable than try_lock() which may fail silently
        let was_opened = self.was_opened.load(Ordering::SeqCst);
        let was_closed = self.was_closed.load(Ordering::SeqCst);

        if was_opened && !was_closed {
            warn!(
                "BrowserController dropped without calling close(). \
                WebDriver session may not be properly cleaned up. \
                Consider calling close() explicitly before dropping."
            );
        }
    }
}

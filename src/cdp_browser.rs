//! CDP (Chrome DevTools Protocol) browser controller module.
//!
//! This module provides direct browser control using CDP without requiring WebDriver.
//! It uses the chromiumoxide library for native CDP communication.
//! Supports Chrome and Edge browsers (both are Chromium-based).

use crate::browser::EnvState;
use crate::config::Config;
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::input::{DispatchKeyEventParams, DispatchKeyEventType};
use chromiumoxide::cdp::browser_protocol::page::{
    CaptureScreenshotFormat, GetNavigationHistoryParams, NavigateToHistoryEntryParams,
};
use chromiumoxide::handler::viewport::Viewport;
use chromiumoxide::page::ScreenshotParams;
use chromiumoxide::Page;
use futures::StreamExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Delay in milliseconds to wait for page to settle after actions.
const PAGE_SETTLE_DELAY_MS: u64 = 500;

/// Delay in milliseconds after typing actions.
const TYPING_DELAY_MS: u64 = 100;

/// CDP browser controller that wraps chromiumoxide operations.
pub struct CdpBrowserController {
    browser: Arc<Mutex<Option<Browser>>>,
    page: Arc<Mutex<Option<Page>>>,
    config: Config,
    /// Tracks whether the browser was opened (and thus needs cleanup)
    was_opened: AtomicBool,
    /// Tracks whether close() was called
    was_closed: AtomicBool,
}

impl CdpBrowserController {
    /// Create a new CDP browser controller with the given configuration.
    pub fn new(config: Config) -> Self {
        Self {
            browser: Arc::new(Mutex::new(None)),
            page: Arc::new(Mutex::new(None)),
            config,
            was_opened: AtomicBool::new(false),
            was_closed: AtomicBool::new(false),
        }
    }

    /// Initialize and open the browser using CDP.
    /// If auto_start is false and a CDP URL is available, connects to existing browser.
    /// Otherwise, launches a new browser instance.
    pub async fn open(&self) -> Result<EnvState> {
        let mut browser_guard = self.browser.lock().await;
        let mut page_guard = self.page.lock().await;

        if browser_guard.is_some() {
            // Browser already open, just return current state
            drop(browser_guard);
            drop(page_guard);
            return self.current_state().await;
        }

        // If auto_start is false and we have a CDP URL, connect to existing browser
        if !self.config.auto_start {
            if let Some(ref cdp_url) = self.config.cdp_url {
                drop(browser_guard);
                drop(page_guard);
                return self.connect(cdp_url).await;
            }
        }

        info!("Opening browser via CDP...");

        // Build browser configuration
        let mut builder = BrowserConfig::builder()
            .viewport(Viewport {
                width: self.config.screen_width,
                height: self.config.screen_height,
                device_scale_factor: None,
                emulating_mobile: false,
                is_landscape: false,
                has_touch: false,
            })
            .disable_default_args()
            .arg("--disable-extensions")
            .arg("--disable-plugins")
            .arg("--disable-dev-shm-usage")
            .arg("--disable-background-networking")
            .arg("--disable-default-apps")
            .arg("--disable-sync")
            .arg("--no-first-run")
            .arg("--disable-popup-blocking");

        if self.config.headless {
            builder = builder.arg("--headless=new").arg("--no-sandbox");
        }

        // Undetected mode settings
        if self.config.undetected {
            info!("Enabling undetected mode");
            builder = builder
                .arg("--disable-blink-features=AutomationControlled")
                .arg("--disable-infobars")
                .arg("--disable-notifications");
        }

        // Set browser binary if specified
        if let Some(ref binary_path) = self.config.browser_binary_path {
            builder = builder.chrome_executable(binary_path);
        }

        let config = builder.build().map_err(|e| anyhow::anyhow!("{}", e))?;

        // Launch browser
        let (browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to launch browser via CDP: {}", e))?;

        // Spawn handler task
        tokio::spawn(async move {
            while let Some(h) = handler.next().await {
                if h.is_err() {
                    break;
                }
            }
        });

        // Create a new page and navigate to initial URL
        let page = browser
            .new_page(&self.config.initial_url)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create page: {}", e))?;

        // Apply stealth scripts if undetected mode is enabled
        if self.config.undetected {
            let stealth_script = r#"
                Object.defineProperty(navigator, 'webdriver', {
                    get: () => undefined
                });
                
                Object.defineProperty(navigator, 'plugins', {
                    get: () => ([
                        {
                            name: 'Chrome PDF Plugin',
                            filename: 'internal-pdf-viewer',
                            description: 'Portable Document Format',
                            length: 1
                        }
                    ])
                });
                
                Object.defineProperty(navigator, 'languages', {
                    get: () => ['en-US', 'en']
                });
            "#;

            if let Err(e) = page.evaluate(stealth_script).await {
                warn!("Failed to apply stealth script: {}", e);
            }
        }

        *browser_guard = Some(browser);
        *page_guard = Some(page);
        self.was_opened.store(true, Ordering::SeqCst);

        drop(browser_guard);
        drop(page_guard);

        info!("Browser opened successfully via CDP");
        self.current_state().await
    }

    /// Connect to an existing browser via CDP.
    pub async fn connect(&self, cdp_url: &str) -> Result<EnvState> {
        let mut browser_guard = self.browser.lock().await;
        let mut page_guard = self.page.lock().await;

        if browser_guard.is_some() {
            drop(browser_guard);
            drop(page_guard);
            return self.current_state().await;
        }

        info!("Connecting to browser via CDP at: {}", cdp_url);

        let (browser, mut handler) = Browser::connect(cdp_url)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to browser via CDP: {}", e))?;

        // Spawn handler task
        tokio::spawn(async move {
            while let Some(h) = handler.next().await {
                if h.is_err() {
                    break;
                }
            }
        });

        // Get existing pages or create a new one
        let pages = browser
            .pages()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get pages: {}", e))?;

        let page = if let Some(existing_page) = pages.into_iter().next() {
            existing_page
        } else {
            browser
                .new_page(&self.config.initial_url)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create page: {}", e))?
        };

        *browser_guard = Some(browser);
        *page_guard = Some(page);
        self.was_opened.store(true, Ordering::SeqCst);

        drop(browser_guard);
        drop(page_guard);

        info!("Connected to browser via CDP");
        self.current_state().await
    }

    /// Close the browser.
    #[allow(dead_code)]
    pub async fn close(&self) -> Result<()> {
        let mut browser_guard = self.browser.lock().await;
        let mut page_guard = self.page.lock().await;

        *page_guard = None;
        if let Some(browser) = browser_guard.take() {
            drop(browser);
            self.was_closed.store(true, Ordering::SeqCst);
            info!("Browser closed");
        }

        Ok(())
    }

    /// Get the current page reference.
    async fn get_page(&self) -> Result<Page> {
        let page_guard = self.page.lock().await;
        page_guard
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Browser not opened"))
    }

    /// Get the current state (screenshot and URL).
    pub async fn current_state(&self) -> Result<EnvState> {
        let page = self.get_page().await?;

        // Wait for page to be ready
        tokio::time::sleep(Duration::from_millis(PAGE_SETTLE_DELAY_MS)).await;

        // Take screenshot
        let screenshot_bytes = page
            .screenshot(
                ScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Png)
                    .build(),
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to take screenshot: {}", e))?;

        let screenshot = BASE64.encode(&screenshot_bytes);
        let url = page
            .url()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get URL: {}", e))?
            .unwrap_or_else(|| "about:blank".to_string());

        Ok(EnvState { screenshot, url })
    }

    /// Click at specific coordinates.
    pub async fn click_at(&self, x: i64, y: i64) -> Result<EnvState> {
        debug!("Clicking at ({}, {})", x, y);
        let page = self.get_page().await?;

        // Use JavaScript to click at coordinates
        let script = format!(
            r#"
            (function() {{
                var element = document.elementFromPoint({}, {});
                if (element) {{
                    element.click();
                    return true;
                }}
                return false;
            }})();
            "#,
            x, y
        );

        page.evaluate(script)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to click: {}", e))?;

        tokio::time::sleep(Duration::from_millis(PAGE_SETTLE_DELAY_MS)).await;
        self.current_state().await
    }

    /// Hover at specific coordinates.
    pub async fn hover_at(&self, x: i64, y: i64) -> Result<EnvState> {
        debug!("Hovering at ({}, {})", x, y);
        let page = self.get_page().await?;

        let script = format!(
            r#"
            (function() {{
                var element = document.elementFromPoint({}, {});
                if (element) {{
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

        page.evaluate(script)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to hover: {}", e))?;

        tokio::time::sleep(Duration::from_millis(PAGE_SETTLE_DELAY_MS)).await;
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
        debug!("Typing at ({}, {}): {}", x, y, text);
        let page = self.get_page().await?;

        // Click to focus element
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
        page.evaluate(click_script)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to focus: {}", e))?;

        tokio::time::sleep(Duration::from_millis(TYPING_DELAY_MS)).await;

        if clear_before_typing {
            // Clear content using appropriate method for each element type
            let clear_script = r#"
                var active = document.activeElement;
                if (active && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA')) {
                    active.value = '';
                    active.dispatchEvent(new Event('input', { bubbles: true }));
                } else if (active && active.isContentEditable) {
                    // Use Selection API for contentEditable elements
                    var selection = window.getSelection();
                    var range = document.createRange();
                    range.selectNodeContents(active);
                    selection.removeAllRanges();
                    selection.addRange(range);
                    selection.deleteFromDocument();
                }
            "#;
            page.evaluate(clear_script)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to clear: {}", e))?;
        }

        // Type text using JavaScript
        let escaped_text = serde_json::to_string(text)
            .unwrap_or_else(|_| format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\"")));
        let type_script = format!(
            r#"
            (function() {{
                var text = {};
                var active = document.activeElement;
                if (active && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA')) {{
                    active.value = (active.value || '') + text;
                    active.dispatchEvent(new Event('input', {{ bubbles: true }}));
                }} else if (active && active.isContentEditable) {{
                    // Replace selection with text node in contentEditable elements
                    var selection = window.getSelection();
                    if (selection.rangeCount > 0) {{
                        var range = selection.getRangeAt(0);
                        range.deleteContents();
                        var textNode = document.createTextNode(text);
                        range.insertNode(textNode);
                        // Move cursor to end of inserted text
                        range.setStartAfter(textNode);
                        range.setEndAfter(textNode);
                        selection.removeAllRanges();
                        selection.addRange(range);
                    }} else {{
                        active.textContent += text;
                    }}
                }} else {{
                    // Fallback: dispatch key events
                    for (var i = 0; i < text.length; i++) {{
                        var event = new KeyboardEvent('keypress', {{
                            key: text[i],
                            charCode: text.charCodeAt(i),
                            bubbles: true
                        }});
                        active.dispatchEvent(event);
                    }}
                }}
            }})();
            "#,
            escaped_text
        );
        page.evaluate(type_script)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to type: {}", e))?;

        if press_enter {
            // Use CDP to send Enter key
            let enter_params = DispatchKeyEventParams::builder()
                .r#type(DispatchKeyEventType::KeyDown)
                .key("Enter")
                .code("Enter")
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build Enter key params: {}", e))?;
            page.execute(enter_params)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to press Enter: {}", e))?;

            let enter_up_params = DispatchKeyEventParams::builder()
                .r#type(DispatchKeyEventType::KeyUp)
                .key("Enter")
                .code("Enter")
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build Enter key up params: {}", e))?;
            page.execute(enter_up_params)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to release Enter: {}", e))?;
        }

        tokio::time::sleep(Duration::from_millis(PAGE_SETTLE_DELAY_MS)).await;
        self.current_state().await
    }

    /// Scroll the entire document.
    pub async fn scroll_document(&self, direction: &str) -> Result<EnvState> {
        debug!("Scrolling document: {}", direction);
        let page = self.get_page().await?;

        let script = match direction.to_lowercase().as_str() {
            "up" => "window.scrollBy(0, -window.innerHeight * 0.8);",
            "down" => "window.scrollBy(0, window.innerHeight * 0.8);",
            "left" => "window.scrollBy(-window.innerWidth * 0.5, 0);",
            "right" => "window.scrollBy(window.innerWidth * 0.5, 0);",
            _ => return Err(anyhow::anyhow!("Invalid scroll direction: {}", direction)),
        };

        page.evaluate(script)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to scroll: {}", e))?;

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
        debug!(
            "Scrolling at ({}, {}) direction: {} magnitude: {}",
            x, y, direction, magnitude
        );
        let page = self.get_page().await?;

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

        page.evaluate(script)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to scroll: {}", e))?;

        self.current_state().await
    }

    /// Wait for 5 seconds.
    pub async fn wait_5_seconds(&self) -> Result<EnvState> {
        debug!("Waiting 5 seconds");
        tokio::time::sleep(Duration::from_secs(5)).await;
        self.current_state().await
    }

    /// Navigate back using CDP.
    pub async fn go_back(&self) -> Result<EnvState> {
        debug!("Going back");
        let page = self.get_page().await?;

        // Get navigation history
        let history = page
            .execute(GetNavigationHistoryParams::default())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get navigation history: {}", e))?;

        let current_index = history.result.current_index;
        if current_index > 0 {
            let prev_entry = &history.result.entries[(current_index - 1) as usize];
            page.execute(NavigateToHistoryEntryParams::new(prev_entry.id))
                .await
                .map_err(|e| anyhow::anyhow!("Failed to navigate back: {}", e))?;
        }

        tokio::time::sleep(Duration::from_millis(PAGE_SETTLE_DELAY_MS)).await;
        self.current_state().await
    }

    /// Navigate forward using CDP.
    pub async fn go_forward(&self) -> Result<EnvState> {
        debug!("Going forward");
        let page = self.get_page().await?;

        // Get navigation history
        let history = page
            .execute(GetNavigationHistoryParams::default())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get navigation history: {}", e))?;

        let current_index = history.result.current_index as usize;
        if current_index + 1 < history.result.entries.len() {
            let next_entry = &history.result.entries[current_index + 1];
            page.execute(NavigateToHistoryEntryParams::new(next_entry.id))
                .await
                .map_err(|e| anyhow::anyhow!("Failed to navigate forward: {}", e))?;
        }

        tokio::time::sleep(Duration::from_millis(PAGE_SETTLE_DELAY_MS)).await;
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
        let page = self.get_page().await?;

        let normalized_url = if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else {
            format!("https://{}", url)
        };

        page.goto(&normalized_url)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to navigate: {}", e))?;

        // Wait for page to load
        tokio::time::sleep(Duration::from_millis(PAGE_SETTLE_DELAY_MS * 2)).await;
        self.current_state().await
    }

    /// Press key combination using CDP.
    pub async fn key_combination(&self, keys: Vec<String>) -> Result<EnvState> {
        debug!("Pressing key combination: {:?}", keys);
        let page = self.get_page().await?;

        // Use CDP to dispatch key events
        for key in &keys {
            let key_down = DispatchKeyEventParams::builder()
                .r#type(DispatchKeyEventType::KeyDown)
                .key(key.as_str())
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build key down params: {}", e))?;
            page.execute(key_down)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to press key: {}", e))?;

            let key_up = DispatchKeyEventParams::builder()
                .r#type(DispatchKeyEventType::KeyUp)
                .key(key.as_str())
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build key up params: {}", e))?;
            page.execute(key_up)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to release key: {}", e))?;
        }

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
        debug!(
            "Drag and drop from ({}, {}) to ({}, {})",
            x, y, destination_x, destination_y
        );
        let page = self.get_page().await?;

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

        page.evaluate(script)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to drag and drop: {}", e))?;

        self.current_state().await
    }

    /// Get the screen size.
    #[allow(dead_code)]
    pub fn screen_size(&self) -> (u32, u32) {
        (self.config.screen_width, self.config.screen_height)
    }
}

impl Drop for CdpBrowserController {
    fn drop(&mut self) {
        let was_opened = self.was_opened.load(Ordering::SeqCst);
        let was_closed = self.was_closed.load(Ordering::SeqCst);

        if was_opened && !was_closed {
            warn!(
                "CdpBrowserController dropped without calling close(). \
                Browser session may not be properly cleaned up."
            );
        }
    }
}

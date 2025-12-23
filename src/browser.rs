//! Browser controller module using thirtyfour.
//!
//! This module provides browser automation capabilities using WebDriver.

use crate::config::Config;
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use std::sync::Arc;
use std::time::Duration;
use thirtyfour::prelude::*;
use tokio::sync::Mutex;
use tracing::{debug, info};

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

/// Environment state returned by browser actions.
#[derive(Debug, Clone)]
pub struct EnvState {
    /// Screenshot in PNG format, base64 encoded.
    pub screenshot: String,
    /// Current URL of the page.
    pub url: String,
}

/// Validate coordinates are within reasonable screen bounds.
fn validate_coordinates(x: i64, y: i64, width: u32, height: u32) -> Result<()> {
    if x < 0 || y < 0 {
        return Err(anyhow::anyhow!("Coordinates cannot be negative: ({}, {})", x, y));
    }
    if x as u32 > width * 2 || y as u32 > height * 2 {
        return Err(anyhow::anyhow!(
            "Coordinates ({}, {}) are too far outside screen bounds ({}x{})",
            x, y, width, height
        ));
    }
    Ok(())
}

/// Browser controller that wraps WebDriver operations.
pub struct BrowserController {
    driver: Arc<Mutex<Option<WebDriver>>>,
    config: Config,
}

impl BrowserController {
    /// Create a new browser controller with the given configuration.
    pub fn new(config: Config) -> Self {
        Self {
            driver: Arc::new(Mutex::new(None)),
            config,
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

        info!("Opening browser...");

        // Currently only Chrome is fully supported
        let mut caps = DesiredCapabilities::chrome();
        if self.config.headless {
            caps.add_arg("--headless")?;
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
        if let Some(ref binary_path) = self.config.browser_binary_path {
            caps.set_binary(binary_path.to_string_lossy().as_ref())?;
        }

        let driver = WebDriver::new(&self.config.webdriver_url, caps).await?;

        // Set window size
        driver
            .set_window_rect(
                0,
                0,
                self.config.screen_width,
                self.config.screen_height,
            )
            .await?;

        // Navigate to initial URL
        driver.goto(&self.config.initial_url).await?;

        *driver_guard = Some(driver);
        drop(driver_guard);

        info!("Browser opened successfully");
        self.current_state().await
    }

    /// Close the browser.
    pub async fn close(&self) -> Result<()> {
        let mut driver_guard = self.driver.lock().await;
        if let Some(driver) = driver_guard.take() {
            driver.quit().await?;
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

        // Wait a bit for page to settle
        tokio::time::sleep(Duration::from_millis(500)).await;

        let screenshot_bytes = driver.screenshot_as_png().await?;
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

        // Use JavaScript to click at coordinates
        // Note: x and y are i64, so format! only produces numeric values (no injection risk)
        let script = format!(
            r#"
            var element = document.elementFromPoint({}, {});
            if (element) {{
                element.click();
            }} else {{
                var event = new MouseEvent('click', {{
                    view: window,
                    bubbles: true,
                    cancelable: true,
                    clientX: {},
                    clientY: {}
                }});
                document.dispatchEvent(event);
            }}
            "#,
            x, y, x, y
        );
        driver.execute(&script, vec![]).await?;

        // Wait for potential navigation
        tokio::time::sleep(Duration::from_millis(500)).await;

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

        // Use JavaScript to simulate hover
        let script = format!(
            r#"
            var element = document.elementFromPoint({}, {});
            if (element) {{
                var event = new MouseEvent('mouseover', {{
                    view: window,
                    bubbles: true,
                    cancelable: true,
                    clientX: {},
                    clientY: {}
                }});
                element.dispatchEvent(event);
            }}
            "#,
            x, y, x, y
        );
        driver.execute(&script, vec![]).await?;

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
        tokio::time::sleep(Duration::from_millis(100)).await;

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

        tokio::time::sleep(Duration::from_millis(500)).await;

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
        tokio::time::sleep(Duration::from_millis(500)).await;

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
        tokio::time::sleep(Duration::from_millis(500)).await;

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
        tokio::time::sleep(Duration::from_millis(500)).await;

        drop(driver_guard);
        self.current_state().await
    }

    /// Press key combination.
    pub async fn key_combination(&self, keys: Vec<String>) -> Result<EnvState> {
        debug!("Pressing key combination: {:?}", keys);
        let driver_guard = self.driver.lock().await;
        let driver = driver_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Browser not opened"))?;

        if keys.is_empty() {
            return Err(anyhow::anyhow!("No keys provided"));
        }

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
            let ctrl = keys.iter().any(|k| k.to_lowercase() == "control" || k.to_lowercase() == "ctrl");
            let shift = keys.iter().any(|k| k.to_lowercase() == "shift");
            let alt = keys.iter().any(|k| k.to_lowercase() == "alt");
            let meta = keys.iter().any(|k| k.to_lowercase() == "meta" || k.to_lowercase() == "command");

            // Find the main key (non-modifier)
            let main_key = keys.iter().find(|k| {
                let lower = k.to_lowercase();
                !["control", "ctrl", "shift", "alt", "meta", "command"].contains(&lower.as_str())
            });

            if let Some(key) = main_key {
                let script = format!(
                    r#"
                    var event = new KeyboardEvent('keydown', {{
                        key: '{}',
                        ctrlKey: {},
                        shiftKey: {},
                        altKey: {},
                        metaKey: {},
                        bubbles: true
                    }});
                    document.activeElement.dispatchEvent(event);
                    "#,
                    key.to_lowercase(),
                    ctrl,
                    shift,
                    alt,
                    meta
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
        validate_coordinates(destination_x, destination_y, self.config.screen_width, self.config.screen_height)?;
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

    /// Get the screen size.
    pub fn screen_size(&self) -> (u32, u32) {
        (self.config.screen_width, self.config.screen_height)
    }
}

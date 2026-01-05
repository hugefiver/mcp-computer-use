//! Browser management module.
//!
//! This module provides functionality for auto-detecting browsers,
//! launching browsers with CDP (Chrome DevTools Protocol) support,
//! and managing browser processes.

use crate::config::{BrowserType, Config};
use anyhow::{Context, Result};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Maximum time to wait for browser to become ready (in seconds).
const BROWSER_READY_TIMEOUT_SECS: u64 = 30;

/// Interval between health checks (in milliseconds).
const HEALTH_CHECK_INTERVAL_MS: u64 = 100;

/// Common Chrome browser paths on different platforms.
#[cfg(target_os = "windows")]
const CHROME_PATHS: &[&str] = &[
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Users\%USERNAME%\AppData\Local\Google\Chrome\Application\chrome.exe",
];

#[cfg(target_os = "macos")]
const CHROME_PATHS: &[&str] = &[
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
];

#[cfg(target_os = "linux")]
const CHROME_PATHS: &[&str] = &[
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
    "/snap/bin/chromium",
    "/opt/google/chrome/chrome",
];

/// Common ChromeDriver paths on different platforms.
#[cfg(target_os = "windows")]
const CHROMEDRIVER_PATHS: &[&str] = &[
    r"C:\chromedriver\chromedriver.exe",
    r"C:\Program Files\chromedriver\chromedriver.exe",
    r"C:\webdrivers\chromedriver.exe",
];

#[cfg(target_os = "macos")]
const CHROMEDRIVER_PATHS: &[&str] = &[
    "/usr/local/bin/chromedriver",
    "/opt/homebrew/bin/chromedriver",
    "/usr/bin/chromedriver",
];

#[cfg(target_os = "linux")]
const CHROMEDRIVER_PATHS: &[&str] = &[
    "/usr/bin/chromedriver",
    "/usr/local/bin/chromedriver",
    "/snap/bin/chromedriver",
    "/opt/chromedriver/chromedriver",
];

/// Manages browser processes and provides auto-detection capabilities.
pub struct BrowserManager {
    /// The browser process if we launched it.
    browser_process: Option<Child>,
    /// The CDP port being used.
    cdp_port: u16,
}

impl BrowserManager {
    /// Create a new BrowserManager.
    pub fn new() -> Self {
        Self {
            browser_process: None,
            cdp_port: 9222,
        }
    }

    /// Find Chrome browser binary path.
    ///
    /// Search order:
    /// 1. Explicit path from config
    /// 2. PATH environment variable
    /// 3. Common installation paths for the platform
    pub fn find_browser(&self, config: &Config) -> Result<PathBuf> {
        // 1. Check explicit path from config
        if let Some(ref path) = config.browser_binary_path {
            if path.exists() {
                debug!("Using browser from config: {:?}", path);
                return Ok(path.clone());
            }
            warn!(
                "Specified browser path {:?} does not exist, searching elsewhere",
                path
            );
        }

        // 2. Try to find in PATH
        let browser_name = match config.browser_type {
            BrowserType::Chrome => {
                #[cfg(target_os = "windows")]
                {
                    "chrome.exe"
                }
                #[cfg(not(target_os = "windows"))]
                {
                    "google-chrome"
                }
            }
            BrowserType::Firefox => {
                #[cfg(target_os = "windows")]
                {
                    "firefox.exe"
                }
                #[cfg(not(target_os = "windows"))]
                {
                    "firefox"
                }
            }
            BrowserType::Edge => {
                #[cfg(target_os = "windows")]
                {
                    "msedge.exe"
                }
                #[cfg(not(target_os = "windows"))]
                {
                    "microsoft-edge"
                }
            }
            BrowserType::Safari => "safari",
        };

        // Try multiple browser names for Chrome
        let browser_names: Vec<&str> = match config.browser_type {
            BrowserType::Chrome => {
                #[cfg(target_os = "windows")]
                {
                    vec!["chrome.exe"]
                }
                #[cfg(target_os = "macos")]
                {
                    vec!["Google Chrome", "Chromium", "chromium"]
                }
                #[cfg(target_os = "linux")]
                {
                    vec![
                        "google-chrome",
                        "google-chrome-stable",
                        "chromium",
                        "chromium-browser",
                    ]
                }
            }
            _ => vec![browser_name],
        };

        for name in &browser_names {
            if let Ok(path) = which::which(name) {
                debug!("Found browser in PATH: {:?}", path);
                return Ok(path);
            }
        }

        // 3. Check common installation paths
        let common_paths: &[&str] = match config.browser_type {
            BrowserType::Chrome => CHROME_PATHS,
            _ => &[],
        };

        for path_str in common_paths {
            // Expand environment variables on Windows
            #[cfg(target_os = "windows")]
            let path_str = {
                if path_str.contains('%') {
                    if let Ok(expanded) = std::env::var("USERNAME") {
                        path_str.replace("%USERNAME%", &expanded)
                    } else {
                        path_str.to_string()
                    }
                } else {
                    path_str.to_string()
                }
            };
            #[cfg(not(target_os = "windows"))]
            let path_str = path_str.to_string();

            let path = PathBuf::from(&path_str);
            if path.exists() {
                debug!("Found browser at common path: {:?}", path);
                return Ok(path);
            }
        }

        Err(anyhow::anyhow!(
            "Could not find {} browser. Please install it or set MCP_BROWSER_BINARY_PATH.",
            browser_name
        ))
    }

    /// Find ChromeDriver binary path.
    ///
    /// Search order:
    /// 1. Explicit path from config
    /// 2. PATH environment variable
    /// 3. Common installation paths for the platform
    pub fn find_driver(&self, config: &Config) -> Result<PathBuf> {
        // 1. Check explicit path from config
        if let Some(ref path) = config.driver_path {
            if path.exists() {
                debug!("Using driver from config: {:?}", path);
                return Ok(path.clone());
            }
            warn!(
                "Specified driver path {:?} does not exist, searching elsewhere",
                path
            );
        }

        // 2. Try to find in PATH
        let driver_name = match config.browser_type {
            BrowserType::Chrome => "chromedriver",
            BrowserType::Firefox => "geckodriver",
            BrowserType::Edge => "msedgedriver",
            BrowserType::Safari => "safaridriver",
        };

        #[cfg(target_os = "windows")]
        let driver_name = match config.browser_type {
            BrowserType::Chrome => "chromedriver.exe",
            BrowserType::Firefox => "geckodriver.exe",
            BrowserType::Edge => "msedgedriver.exe",
            BrowserType::Safari => "safaridriver.exe",
        };

        if let Ok(path) = which::which(driver_name) {
            debug!("Found driver in PATH: {:?}", path);
            return Ok(path);
        }

        // 3. Check common installation paths
        let common_paths: &[&str] = match config.browser_type {
            BrowserType::Chrome => CHROMEDRIVER_PATHS,
            _ => &[],
        };

        for path_str in common_paths {
            let path = PathBuf::from(path_str);
            if path.exists() {
                debug!("Found driver at common path: {:?}", path);
                return Ok(path);
            }
        }

        Err(anyhow::anyhow!(
            "Could not find {} in PATH or common locations. \
            Please install it or set MCP_DRIVER_PATH.",
            driver_name
        ))
    }

    /// Launch Chrome browser with CDP (Chrome DevTools Protocol) enabled.
    ///
    /// Returns the CDP WebSocket URL for connecting.
    pub fn launch_browser_with_cdp(&mut self, config: &Config) -> Result<String> {
        let browser_path = self.find_browser(config)?;
        self.cdp_port = config.cdp_port;

        info!(
            "Launching browser with CDP on port {}: {:?}",
            self.cdp_port, browser_path
        );

        let mut cmd = Command::new(&browser_path);

        // Essential CDP arguments
        cmd.arg(format!("--remote-debugging-port={}", self.cdp_port));

        // Standard Chrome arguments
        cmd.arg("--disable-extensions");
        cmd.arg("--disable-plugins");
        cmd.arg("--disable-dev-shm-usage");
        cmd.arg("--disable-background-networking");
        cmd.arg("--disable-default-apps");
        cmd.arg("--disable-sync");
        cmd.arg("--no-sandbox");
        cmd.arg("--no-first-run");
        cmd.arg("--disable-popup-blocking");
        cmd.arg(format!(
            "--window-size={},{}",
            config.screen_width, config.screen_height
        ));

        if config.headless {
            cmd.arg("--headless=new");
        }

        // Undetected mode
        if config.undetected {
            cmd.arg("--disable-blink-features=AutomationControlled");
            cmd.arg("--disable-infobars");
            cmd.arg("--disable-notifications");
        }

        // Open with initial URL
        cmd.arg(&config.initial_url);

        // Suppress output
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        let child = cmd
            .spawn()
            .with_context(|| format!("Failed to launch browser from {:?}", browser_path))?;

        self.browser_process = Some(child);

        // Wait for browser to become ready
        self.wait_for_cdp_ready()?;

        let cdp_url = format!("http://127.0.0.1:{}", self.cdp_port);
        info!("Browser launched and CDP ready at {}", cdp_url);

        Ok(cdp_url)
    }

    /// Wait for CDP endpoint to become ready.
    fn wait_for_cdp_ready(&self) -> Result<()> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(BROWSER_READY_TIMEOUT_SECS);
        let addr = format!("127.0.0.1:{}", self.cdp_port);

        debug!("Waiting for CDP to become ready on port {}", self.cdp_port);

        while start.elapsed() < timeout {
            match TcpStream::connect_timeout(
                &addr.parse().unwrap(),
                Duration::from_millis(HEALTH_CHECK_INTERVAL_MS),
            ) {
                Ok(_) => {
                    debug!("CDP ready after {:?}", start.elapsed());
                    return Ok(());
                }
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(HEALTH_CHECK_INTERVAL_MS));
                }
            }
        }

        Err(anyhow::anyhow!(
            "Browser CDP endpoint failed to become ready within {} seconds",
            BROWSER_READY_TIMEOUT_SECS
        ))
    }

    /// Check if CDP endpoint is available at the specified port.
    pub fn is_cdp_available(&self, port: u16) -> bool {
        let addr = format!("127.0.0.1:{}", port);
        TcpStream::connect_timeout(
            &addr.parse().unwrap(),
            Duration::from_millis(HEALTH_CHECK_INTERVAL_MS),
        )
        .is_ok()
    }

    /// Stop the browser process if we launched it.
    pub fn stop(&mut self) {
        if let Some(mut child) = self.browser_process.take() {
            info!("Stopping browser process");
            if let Err(e) = child.kill() {
                warn!("Failed to kill browser process: {}", e);
            }
            let _ = child.wait();
        }
    }
}

impl Default for BrowserManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for BrowserManager {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_manager_creation() {
        let manager = BrowserManager::new();
        assert!(manager.browser_process.is_none());
        assert_eq!(manager.cdp_port, 9222);
    }
}

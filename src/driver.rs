//! Browser driver management module.
//!
//! This module provides functionality to automatically launch and manage
//! browser drivers like ChromeDriver.

use crate::config::Config;
use anyhow::{Context, Result};
use std::process::{Child, Command};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Manages the lifecycle of a browser driver process.
pub struct DriverManager {
    process: Option<Child>,
    port: u16,
}

impl DriverManager {
    /// Create a new DriverManager without starting a driver.
    pub fn new() -> Self {
        Self {
            process: None,
            port: 9515,
        }
    }

    /// Start the browser driver based on configuration.
    pub fn start(&mut self, config: &Config) -> Result<String> {
        if !config.auto_launch_driver {
            debug!("Auto-launch driver is disabled, using existing webdriver URL");
            return Ok(config.webdriver_url.clone());
        }

        let driver_path = self.find_driver(config)?;
        self.port = config.driver_port;

        info!(
            "Starting browser driver from: {:?} on port {}",
            driver_path, self.port
        );

        let child = Command::new(&driver_path)
            .arg(format!("--port={}", self.port))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .with_context(|| format!("Failed to start driver from {:?}", driver_path))?;

        self.process = Some(child);

        // Wait a bit for the driver to start
        std::thread::sleep(Duration::from_millis(500));

        let url = format!("http://localhost:{}", self.port);
        info!("Browser driver started at {}", url);
        Ok(url)
    }

    /// Find the driver executable.
    fn find_driver(&self, config: &Config) -> Result<std::path::PathBuf> {
        // First, check if a path is explicitly specified
        if let Some(ref path) = config.driver_path {
            if path.exists() {
                return Ok(path.clone());
            }
            warn!(
                "Specified driver path {:?} does not exist, searching in PATH",
                path
            );
        }

        // Try to find the driver in PATH
        let driver_name = match config.browser_type {
            crate::config::BrowserType::Chrome => "chromedriver",
            crate::config::BrowserType::Firefox => "geckodriver",
            crate::config::BrowserType::Edge => "msedgedriver",
            crate::config::BrowserType::Safari => "safaridriver",
        };

        which::which(driver_name).with_context(|| {
            format!(
                "Could not find {} in PATH. Please install it or set MCP_DRIVER_PATH.",
                driver_name
            )
        })
    }

    /// Stop the driver process if running.
    pub fn stop(&mut self) {
        if let Some(mut child) = self.process.take() {
            info!("Stopping browser driver");
            if let Err(e) = child.kill() {
                warn!("Failed to kill driver process: {}", e);
            }
            // Wait for the process to actually exit
            let _ = child.wait();
        }
    }
}

impl Default for DriverManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DriverManager {
    fn drop(&mut self) {
        self.stop();
    }
}

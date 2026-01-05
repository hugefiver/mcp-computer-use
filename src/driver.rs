//! Browser driver management module.
//!
//! This module provides functionality to automatically launch and manage
//! browser drivers like ChromeDriver.

use crate::browser_manager::BrowserManager;
use crate::config::Config;
use anyhow::{Context, Result};
use std::net::TcpStream;
use std::process::{Child, Command};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Maximum time to wait for driver to become ready (in seconds).
const DRIVER_READY_TIMEOUT_SECS: u64 = 30;

/// Interval between health checks (in milliseconds).
const HEALTH_CHECK_INTERVAL_MS: u64 = 100;

/// Manages the lifecycle of a browser driver process.
pub struct DriverManager {
    process: Option<Child>,
    port: u16,
    browser_manager: BrowserManager,
}

impl DriverManager {
    /// Create a new DriverManager without starting a driver.
    pub fn new() -> Self {
        Self {
            process: None,
            port: 9515,
            browser_manager: BrowserManager::new(),
        }
    }

    /// Ensure a browser driver is ready for use.
    ///
    /// If `auto_launch_driver` is enabled in config, this will launch a new driver
    /// and wait for it to become ready. Otherwise, it returns the existing webdriver URL.
    ///
    /// Returns the URL of the ready driver.
    pub fn ensure_driver_ready(&mut self, config: &Config) -> Result<String> {
        if !config.auto_launch_driver {
            debug!("Auto-launch driver is disabled, using existing webdriver URL");
            return Ok(config.webdriver_url.clone());
        }

        // Use enhanced driver finding from browser_manager
        let driver_path = self.browser_manager.find_driver(config)?;
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

        let url = format!("http://localhost:{}", self.port);

        // Wait for the driver to become ready by checking if the port is accepting connections
        self.wait_for_driver_ready()?;

        info!("Browser driver started and ready at {}", url);
        Ok(url)
    }

    /// Wait for the driver to become ready by attempting to connect to its port.
    fn wait_for_driver_ready(&self) -> Result<()> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(DRIVER_READY_TIMEOUT_SECS);
        let addr = format!("127.0.0.1:{}", self.port);

        debug!("Waiting for driver to become ready on port {}", self.port);

        while start.elapsed() < timeout {
            // Try to connect to the driver's port
            match TcpStream::connect_timeout(
                &addr.parse().unwrap(),
                Duration::from_millis(HEALTH_CHECK_INTERVAL_MS),
            ) {
                Ok(_) => {
                    debug!("Driver ready after {:?}", start.elapsed());
                    return Ok(());
                }
                Err(_) => {
                    // Continue waiting - checking process status would require try_wait
                    // which needs mutable reference and adds complexity
                    std::thread::sleep(Duration::from_millis(HEALTH_CHECK_INTERVAL_MS));
                }
            }
        }

        Err(anyhow::anyhow!(
            "Driver failed to become ready within {} seconds",
            DRIVER_READY_TIMEOUT_SECS
        ))
    }

    /// Get a reference to the browser manager.
    pub fn browser_manager(&mut self) -> &mut BrowserManager {
        &mut self.browser_manager
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
        // Also stop browser if we launched it
        self.browser_manager.stop();
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

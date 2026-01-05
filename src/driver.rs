//! Browser driver management module.
//!
//! This module provides functionality to automatically launch, download,
//! and manage browser drivers like ChromeDriver.

use crate::browser_manager::BrowserManager;
use crate::config::{Config, DEFAULT_DRIVER_PORT};
use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Maximum time to wait for driver to become ready (in seconds).
const DRIVER_READY_TIMEOUT_SECS: u64 = 30;

/// Interval between health checks (in milliseconds).
const HEALTH_CHECK_INTERVAL_MS: u64 = 100;

/// Chrome for Testing API endpoint for latest versions.
const CHROME_VERSIONS_URL: &str =
    "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json";

/// Manages the lifecycle of a browser driver process.
pub struct DriverManager {
    /// The driver process if we launched it.
    driver_process: Option<Child>,
    /// The port the driver is running on.
    port: u16,
    /// Browser manager for browser lifecycle.
    browser_manager: BrowserManager,
    /// Path to the driver executable (cached after finding/downloading).
    driver_path: Option<PathBuf>,
}

impl DriverManager {
    /// Create a new DriverManager without starting a driver.
    pub fn new() -> Self {
        Self {
            driver_process: None,
            port: DEFAULT_DRIVER_PORT,
            browser_manager: BrowserManager::new(),
            driver_path: None,
        }
    }

    /// Ensure a browser driver is ready for use.
    ///
    /// Behavior depends on config:
    /// - If `auto_start` is false: returns the configured or default WebDriver URL
    /// - If `auto_start` is true: finds/downloads driver and launches it
    ///
    /// Returns the URL of the ready driver.
    pub fn ensure_driver_ready(&mut self, config: &Config) -> Result<String> {
        if !config.auto_start {
            debug!("Auto-start is disabled, using existing webdriver URL");
            return Ok(config.effective_webdriver_url());
        }

        self.port = config.effective_driver_port();

        // Check if port is already in use
        if self.is_port_in_use(self.port) {
            return Err(anyhow::anyhow!(
                "Port {} is already in use. Please stop the existing process or configure \
                 a different port with MCP_DRIVER_PORT.",
                self.port
            ));
        }

        // Try to find the driver
        let driver_path = self.find_or_download_driver(config)?;
        self.driver_path = Some(driver_path.clone());

        info!(
            "Starting browser driver from: {:?} on port {}",
            driver_path, self.port
        );

        let child = Command::new(&driver_path)
            .arg(format!("--port={}", self.port))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit()) // Inherit stderr for debugging startup issues
            .spawn()
            .with_context(|| format!("Failed to start driver from {:?}", driver_path))?;

        self.driver_process = Some(child);

        let url = format!("http://localhost:{}", self.port);

        // Wait for the driver to become ready
        self.wait_for_driver_ready()?;

        info!("Browser driver started and ready at {}", url);
        Ok(url)
    }

    /// Check if a port is already in use.
    fn is_port_in_use(&self, port: u16) -> bool {
        let addr: std::net::SocketAddr = match format!("127.0.0.1:{}", port).parse() {
            Ok(a) => a,
            Err(_) => return false,
        };
        TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok()
    }

    /// Find the driver in system or download it if enabled.
    fn find_or_download_driver(&self, config: &Config) -> Result<PathBuf> {
        // First, try to find existing driver
        match self.browser_manager.find_driver(config) {
            Ok(path) => {
                info!("Found existing driver at: {:?}", path);
                return Ok(path);
            }
            Err(e) => {
                debug!("Driver not found in system: {}", e);
            }
        }

        // If not found and auto_download is enabled, download it
        if config.auto_download_driver {
            info!("Driver not found, attempting to download...");
            return download_chromedriver_sync();
        }

        Err(anyhow::anyhow!(
            "ChromeDriver not found. Please install it manually, set MCP_DRIVER_PATH, \
            or enable MCP_AUTO_DOWNLOAD_DRIVER=true to download automatically."
        ))
    }

    /// Wait for the driver to become ready by attempting to connect to its port.
    fn wait_for_driver_ready(&self) -> Result<()> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(DRIVER_READY_TIMEOUT_SECS);
        let addr: std::net::SocketAddr = format!("127.0.0.1:{}", self.port)
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid address format: {}", e))?;

        debug!("Waiting for driver to become ready on port {}", self.port);

        while start.elapsed() < timeout {
            match TcpStream::connect_timeout(&addr, Duration::from_millis(HEALTH_CHECK_INTERVAL_MS))
            {
                Ok(_) => {
                    debug!("Driver ready after {:?}", start.elapsed());
                    return Ok(());
                }
                Err(_) => {
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
        if let Some(mut child) = self.driver_process.take() {
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

/// Get the platform string for chromedriver downloads.
fn get_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86_64") {
            "win64"
        } else {
            "win32"
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "mac-arm64"
        } else {
            "mac-x64"
        }
    } else if cfg!(target_os = "linux") {
        if cfg!(target_arch = "x86_64") {
            "linux64"
        } else {
            // Chrome for Testing API doesn't support linux32 or ARM Linux,
            // fall back to linux64 which may not work on non-x86_64 architectures
            warn!("Chrome for Testing API may not support this Linux architecture; attempting linux64");
            "linux64"
        }
    } else {
        "linux64"
    }
}

/// Get the executable file name for chromedriver on the current platform.
fn get_chromedriver_exe_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "chromedriver.exe"
    } else {
        "chromedriver"
    }
}

/// Get the cache directory for downloaded drivers.
fn get_cache_dir() -> Result<PathBuf> {
    // Use the same logic for all platforms - try cache_dir first, then home_dir/.cache
    let cache_dir = dirs::cache_dir()
        .map(|p| p.join("mcp-computer-use"))
        .or_else(|| dirs::home_dir().map(|h| h.join(".cache").join("mcp-computer-use")))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Could not determine cache directory. Please set HOME environment variable \
                 or ensure a standard cache directory is available."
            )
        })?;

    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir)
            .with_context(|| format!("Failed to create cache directory: {:?}", cache_dir))?;
    }

    Ok(cache_dir)
}

/// Download ChromeDriver synchronously.
fn download_chromedriver_sync() -> Result<PathBuf> {
    info!("Downloading ChromeDriver (this may take a while)...");

    // Create a runtime for the async download
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .with_context(|| "Failed to create runtime for driver download")?;

    runtime.block_on(download_chromedriver_async())
}

/// Download ChromeDriver asynchronously.
async fn download_chromedriver_async() -> Result<PathBuf> {
    let platform = get_platform();
    let cache_dir = get_cache_dir()?;

    // Fetch the latest versions JSON
    let client = reqwest::Client::new();
    let response: serde_json::Value = client
        .get(CHROME_VERSIONS_URL)
        .send()
        .await
        .with_context(|| "Failed to fetch Chrome versions")?
        .json()
        .await
        .with_context(|| "Failed to parse Chrome versions JSON")?;

    // Get the stable channel chromedriver download URL
    let stable = response
        .get("channels")
        .and_then(|c| c.get("Stable"))
        .ok_or_else(|| anyhow::anyhow!("Stable channel not found in versions"))?;

    let version = stable
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Version not found"))?;

    let download_url = stable
        .get("downloads")
        .and_then(|d| d.get("chromedriver"))
        .and_then(|cd| cd.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|item| item.get("platform").and_then(|p| p.as_str()) == Some(platform))
        })
        .and_then(|item| item.get("url"))
        .and_then(|u| u.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "ChromeDriver download URL not found for platform: {}",
                platform
            )
        })?;

    info!("Downloading ChromeDriver {} for {}...", version, platform);

    // Create version-specific directory
    let version_dir = cache_dir.join(format!("chromedriver-{}", version));
    if !version_dir.exists() {
        fs::create_dir_all(&version_dir)?;
    }

    // Check if already downloaded
    let exe_name = get_chromedriver_exe_name();
    let exe_path = version_dir.join(exe_name);
    if exe_path.exists() {
        info!("ChromeDriver already cached at: {:?}", exe_path);
        return Ok(exe_path);
    }

    // Use a lock file to prevent concurrent downloads
    let lock_path = version_dir.join(".download.lock");
    let lock_file = fs::File::create(&lock_path)?;

    // Try to acquire exclusive lock (non-blocking check first)
    use std::io::ErrorKind;
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = lock_file.as_raw_fd();
        let result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
        if result != 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == ErrorKind::WouldBlock {
                info!("Another process is downloading ChromeDriver, waiting...");
                // Block until lock is available
                unsafe { libc::flock(fd, libc::LOCK_EX) };
                // Check if download completed while we were waiting
                if exe_path.exists() {
                    info!("ChromeDriver already cached at: {:?}", exe_path);
                    return Ok(exe_path);
                }
            }
        }
    }
    #[cfg(windows)]
    {
        // On Windows, just proceed - file creation will fail if another process has it locked
        drop(&lock_file);
    }

    // Download the zip file
    let zip_response = client
        .get(download_url)
        .send()
        .await
        .with_context(|| "Failed to download ChromeDriver")?;

    let zip_bytes = zip_response
        .bytes()
        .await
        .with_context(|| "Failed to read ChromeDriver download")?;

    // Save zip to temp file
    let zip_path = version_dir.join("chromedriver.zip");
    let mut zip_file = fs::File::create(&zip_path)?;
    zip_file.write_all(&zip_bytes)?;
    drop(zip_file);

    // Extract the zip
    let zip_file = fs::File::open(&zip_path)?;
    let mut archive =
        zip::ZipArchive::new(zip_file).with_context(|| "Failed to open ChromeDriver zip")?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let file_name = file.name().to_string();

        // Find the chromedriver executable - check if filename ends with exe name
        // Handle both flat structure (chromedriver) and nested (chromedriver-linux64/chromedriver)
        let is_chromedriver = if let Some(basename) = file_name.split('/').next_back() {
            basename == exe_name && !file.is_dir()
        } else {
            false
        };

        if is_chromedriver {
            let mut exe_file = fs::File::create(&exe_path)?;
            std::io::copy(&mut file, &mut exe_file)?;

            // Make executable on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&exe_path)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&exe_path, perms)?;
            }

            break;
        }
    }

    // Clean up zip and lock file
    let _ = fs::remove_file(&zip_path);
    let _ = fs::remove_file(&lock_path);

    if exe_path.exists() {
        info!("ChromeDriver downloaded to: {:?}", exe_path);
        Ok(exe_path)
    } else {
        Err(anyhow::anyhow!(
            "Failed to extract chromedriver from downloaded zip"
        ))
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

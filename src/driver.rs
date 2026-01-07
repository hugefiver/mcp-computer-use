//! Browser driver management module.
//!
//! This module provides functionality to automatically launch, download,
//! and manage browser drivers like ChromeDriver, EdgeDriver, and GeckoDriver.

use crate::browser_manager::BrowserManager;
use crate::config::{BrowserType, Config, DEFAULT_DRIVER_PORT};
use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Maximum time to wait for driver to become ready (in seconds).
const DRIVER_READY_TIMEOUT_SECS: u64 = 30;

/// Interval between health checks (in milliseconds).
const HEALTH_CHECK_INTERVAL_MS: u64 = 100;

/// Chrome for Testing API endpoint for latest versions.
const CHROME_VERSIONS_URL: &str =
    "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json";

/// Chrome for Testing API endpoint for known good versions (for version matching).
const CHROME_KNOWN_GOOD_VERSIONS_URL: &str =
    "https://googlechromelabs.github.io/chrome-for-testing/known-good-versions-with-downloads.json";

/// GitHub API endpoint for geckodriver releases.
const GECKODRIVER_RELEASES_URL: &str =
    "https://api.github.com/repos/mozilla/geckodriver/releases/latest";

/// Microsoft Edge WebDriver download page (we construct URLs based on version).
const MSEDGEDRIVER_BASE_URL: &str =
    "https://msedgewebdriverstorage.blob.core.windows.net/edgewebdriver";

/// Fallback base URL for direct GeckoDriver downloads.
const GECKODRIVER_LATEST_DOWNLOAD_BASE_URL: &str =
    "https://github.com/mozilla/geckodriver/releases/latest/download";

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

            // Try to detect the browser version
            let browser_version = match self.browser_manager.find_browser(config) {
                Ok(browser_path) => {
                    match detect_browser_version(&browser_path, config.browser_type) {
                        Ok(version) => {
                            info!(
                                "Detected {:?} browser version: {}",
                                config.browser_type, version
                            );
                            Some(version)
                        }
                        Err(e) => {
                            warn!(
                                "Could not detect {:?} version: {}. Will download latest stable.",
                                config.browser_type, e
                            );
                            None
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "Could not find {:?} browser: {}. Will download latest stable driver.",
                        config.browser_type, e
                    );
                    None
                }
            };

            return download_driver_sync(config.browser_type, browser_version.as_deref());
        }

        let driver_name = match config.browser_type {
            BrowserType::Chrome => "ChromeDriver",
            BrowserType::Edge => "EdgeDriver",
            BrowserType::Firefox => "GeckoDriver",
            BrowserType::Safari => "SafariDriver",
        };

        Err(anyhow::anyhow!(
            "{} not found. Please install it manually, set MCP_DRIVER_PATH, \
            or enable MCP_AUTO_DOWNLOAD_DRIVER=true to download automatically.",
            driver_name
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
fn get_platform_chrome() -> &'static str {
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

/// Get the platform string for msedgedriver downloads.
fn get_platform_edge() -> &'static str {
    if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86_64") {
            "win64"
        } else if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "win32"
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "mac64_m1"
        } else {
            "mac64"
        }
    } else {
        // Linux and other platforms
        "linux64"
    }
}

/// Get the platform string for geckodriver downloads.
fn get_platform_geckodriver() -> &'static str {
    if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86_64") {
            "win64"
        } else if cfg!(target_arch = "aarch64") {
            "win-aarch64"
        } else {
            "win32"
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "macos-aarch64"
        } else {
            "macos"
        }
    } else if cfg!(target_os = "linux") {
        if cfg!(target_arch = "x86_64") {
            "linux64"
        } else if cfg!(target_arch = "aarch64") {
            "linux-aarch64"
        } else {
            "linux32"
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

/// Get the executable file name for msedgedriver on the current platform.
fn get_msedgedriver_exe_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "msedgedriver.exe"
    } else {
        "msedgedriver"
    }
}

/// Get the executable file name for geckodriver on the current platform.
fn get_geckodriver_exe_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "geckodriver.exe"
    } else {
        "geckodriver"
    }
}

/// Validate driver component strings to prevent path traversal.
fn validate_driver_component(name: &str, value: &str) -> Result<()> {
    if value.is_empty() || value.contains(['/', '\\']) {
        return Err(anyhow::anyhow!("Invalid {} value: '{}'", name, value));
    }
    Ok(())
}

/// Build the Edge WebDriver download URL.
fn build_msedgedriver_download_url(version: &str, platform: &str) -> Result<String> {
    validate_driver_component("Edge version", version)?;
    validate_driver_component("Edge platform", platform)?;
    Ok(format!(
        "{}/{}/edgedriver_{}.zip",
        MSEDGEDRIVER_BASE_URL, version, platform
    ))
}

/// Build the GeckoDriver archive name for the current platform.
fn build_geckodriver_archive_name(version: &str, platform: &str) -> Result<String> {
    validate_driver_component("GeckoDriver version", version)?;
    validate_driver_component("GeckoDriver platform", platform)?;
    let extension = if platform.starts_with("win") {
        "zip"
    } else {
        "tar.gz"
    };
    Ok(format!(
        "geckodriver-{}-{}.{}",
        version, platform, extension
    ))
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

/// Detect browser version from the binary.
///
/// Returns the version string (e.g., "120.0.6099.109") or an error if detection fails.
fn detect_browser_version(browser_path: &PathBuf, browser_type: BrowserType) -> Result<String> {
    // Run the browser with --version flag to get version info
    let output = Command::new(browser_path)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("Failed to run browser with --version: {:?}", browser_path))?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Browser --version command failed with status: {}",
            output.status
        ));
    }

    let version_output = String::from_utf8_lossy(&output.stdout);
    debug!("Browser version output: {}", version_output);

    // Parse version from output like:
    // - "Google Chrome 120.0.6099.109"
    // - "Microsoft Edge 120.0.2210.91"
    // - "Mozilla Firefox 121.0"
    // The version is typically the last space-separated token that looks like a version number
    let version = version_output
        .split_whitespace()
        .find(|s| {
            // Version should start with a digit and contain dots
            s.chars().next().is_some_and(|c| c.is_ascii_digit()) && s.contains('.')
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Could not parse version from {:?} browser output: {}",
                browser_type,
                version_output
            )
        })?;

    Ok(version.to_string())
}

/// Extract major version from a full version string.
///
/// E.g., "120.0.6099.109" -> "120"
fn extract_major_version(version: &str) -> Option<&str> {
    version.split('.').next()
}

/// Get the latest stable ChromeDriver version and download URL.
async fn get_latest_stable_chromedriver(
    client: &reqwest::Client,
    platform: &str,
) -> Result<(String, String)> {
    let response: serde_json::Value = client
        .get(CHROME_VERSIONS_URL)
        .send()
        .await
        .with_context(|| "Failed to fetch Chrome versions")?
        .json()
        .await
        .with_context(|| "Failed to parse Chrome versions JSON")?;

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

    Ok((version.to_string(), download_url.to_string()))
}

/// Find a ChromeDriver version matching the browser version.
///
/// Uses the Chrome for Testing known-good-versions API to find a ChromeDriver
/// with the same major version as the browser.
async fn find_matching_chromedriver_version(
    client: &reqwest::Client,
    browser_version: &str,
    platform: &str,
) -> Result<(String, String)> {
    let browser_major = extract_major_version(browser_version).ok_or_else(|| {
        anyhow::anyhow!("Could not extract major version from: {}", browser_version)
    })?;

    debug!(
        "Looking for ChromeDriver matching browser major version: {}",
        browser_major
    );

    // Fetch the known good versions JSON
    let response: serde_json::Value = client
        .get(CHROME_KNOWN_GOOD_VERSIONS_URL)
        .send()
        .await
        .with_context(|| "Failed to fetch known good Chrome versions")?
        .json()
        .await
        .with_context(|| "Failed to parse known good versions JSON")?;

    let versions = response
        .get("versions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("Versions array not found in response"))?;

    // Find the best matching version (highest version with same major)
    // Iterate in reverse since versions are typically sorted ascending
    let matching_version = versions
        .iter()
        .rev()
        .find(|v| {
            let ver = v.get("version").and_then(|v| v.as_str()).unwrap_or("");
            extract_major_version(ver) == Some(browser_major)
                && v.get("downloads")
                    .and_then(|d| d.get("chromedriver"))
                    .is_some()
        })
        .ok_or_else(|| {
            anyhow::anyhow!("No ChromeDriver found for major version {}", browser_major)
        })?;

    let version = matching_version
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Version not found in matching entry"))?;

    let download_url = matching_version
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

    Ok((version.to_string(), download_url.to_string()))
}

/// Download ChromeDriver synchronously.
///
/// This function handles being called from different contexts:
/// - From outside any runtime: creates a new runtime
/// - From a multi-threaded runtime: uses block_in_place
/// - From a single-threaded runtime: spawns an OS thread to avoid blocking
///
/// If `browser_version` is provided, attempts to download a ChromeDriver matching that version.
/// If not provided or matching fails, downloads the latest stable version.
fn download_chromedriver_sync(browser_version: Option<&str>) -> Result<PathBuf> {
    info!("Downloading ChromeDriver (this may take a while)...");

    let version_owned = browser_version.map(|s| s.to_string());

    // Check if we're already inside a Tokio runtime
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            // We're inside an existing runtime
            // Check runtime flavor to determine safe blocking strategy
            match handle.runtime_flavor() {
                tokio::runtime::RuntimeFlavor::MultiThread => {
                    // Multi-threaded runtime: block_in_place is safe
                    tokio::task::block_in_place(|| {
                        handle.block_on(download_chromedriver_async(version_owned.as_deref()))
                    })
                }
                tokio::runtime::RuntimeFlavor::CurrentThread => {
                    // Single-threaded runtime: spawn an OS thread to avoid blocking the runtime
                    std::thread::spawn(move || {
                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .with_context(|| "Failed to create runtime for driver download")?;
                        rt.block_on(download_chromedriver_async(version_owned.as_deref()))
                    })
                    .join()
                    .map_err(|_| {
                        anyhow::anyhow!(
                            "ChromeDriver download failed: thread panicked during execution"
                        )
                    })?
                }
                // Handle any future runtime flavors by falling back to the safe OS thread approach
                _ => std::thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .with_context(|| "Failed to create runtime for driver download")?;
                    rt.block_on(download_chromedriver_async(version_owned.as_deref()))
                })
                .join()
                .map_err(|_| {
                    anyhow::anyhow!(
                        "ChromeDriver download failed: thread panicked during execution"
                    )
                })?,
            }
        }
        Err(_) => {
            // Not in a runtime, create a new one for the async download
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .with_context(|| "Failed to create runtime for driver download")?;

            runtime.block_on(download_chromedriver_async(version_owned.as_deref()))
        }
    }
}

/// Download ChromeDriver asynchronously.
///
/// If `browser_version` is provided, attempts to find a ChromeDriver matching that version.
/// If not provided or no match found, downloads the latest stable version.
async fn download_chromedriver_async(browser_version: Option<&str>) -> Result<PathBuf> {
    let platform = get_platform_chrome();
    let cache_dir = get_cache_dir()?;
    let client = reqwest::Client::new();

    // Try to find a matching version if browser_version is provided
    let (version, download_url) = if let Some(browser_ver) = browser_version {
        match find_matching_chromedriver_version(&client, browser_ver, platform).await {
            Ok((ver, url)) => {
                info!(
                    "Found matching ChromeDriver version {} for browser {}",
                    ver, browser_ver
                );
                (ver, url)
            }
            Err(e) => {
                warn!("Could not find matching ChromeDriver for browser version {}: {}. Falling back to latest stable.", browser_ver, e);
                get_latest_stable_chromedriver(&client, platform).await?
            }
        }
    } else {
        get_latest_stable_chromedriver(&client, platform).await?
    };

    info!("Downloading ChromeDriver {} for {}...", version, platform);

    // Create version-specific directory
    let version_dir = cache_dir.join(format!("chromedriver-{}", version));
    if !version_dir.exists() {
        fs::create_dir_all(&version_dir)?;
    }

    let exe_name = get_chromedriver_exe_name();
    let exe_path = version_dir.join(exe_name);

    // Use a lock file to prevent concurrent downloads
    // The lock file is cleaned up when _lock_guard goes out of scope (including on errors)
    let lock_path = version_dir.join(".download.lock");
    let lock_file = fs::File::create(&lock_path)?;

    // RAII guard to ensure lock file cleanup on all exit paths
    // We store the File object to keep the file descriptor valid for the entire lock duration
    struct LockGuard {
        lock_path: PathBuf,
        #[allow(dead_code)] // File is kept alive to maintain lock
        lock_file: fs::File,
        #[cfg(unix)]
        fd: std::os::unix::io::RawFd,
    }

    impl Drop for LockGuard {
        fn drop(&mut self) {
            // Release the lock explicitly on Unix
            #[cfg(unix)]
            {
                unsafe { libc::flock(self.fd, libc::LOCK_UN) };
            }
            // On Windows, the lock is released when the file handle is closed (which happens
            // when lock_file is dropped after this)
            // Remove the lock file
            let _ = fs::remove_file(&self.lock_path);
        }
    }

    #[cfg(unix)]
    let _lock_guard = {
        use std::os::unix::io::AsRawFd;
        let fd = lock_file.as_raw_fd();

        // Try to acquire exclusive lock (non-blocking check first)
        let result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
        if result != 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::WouldBlock {
                info!("Another process is downloading ChromeDriver, waiting...");
                // Block until lock is available
                let block_result = unsafe { libc::flock(fd, libc::LOCK_EX) };
                if block_result != 0 {
                    let block_err = std::io::Error::last_os_error();
                    return Err(anyhow::anyhow!(
                        "Failed to acquire download lock: {}",
                        block_err
                    ));
                }
            } else {
                return Err(anyhow::anyhow!("Failed to acquire download lock: {}", err));
            }
        }

        LockGuard {
            lock_path: lock_path.clone(),
            lock_file,
            fd,
        }
    };

    #[cfg(windows)]
    let _lock_guard = {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::HANDLE;
        use windows_sys::Win32::Storage::FileSystem::{
            LockFileEx, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
        };

        let handle = lock_file.as_raw_handle() as HANDLE;

        // Try non-blocking lock first
        let mut overlapped: windows_sys::Win32::System::IO::OVERLAPPED =
            unsafe { std::mem::zeroed() };
        let result = unsafe {
            LockFileEx(
                handle,
                LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                0,
                1,
                0,
                &mut overlapped,
            )
        };

        if result == 0 {
            // Lock failed, try blocking
            info!("Another process is downloading ChromeDriver, waiting...");
            // Reinitialize OVERLAPPED before the blocking call
            let mut blocking_overlapped: windows_sys::Win32::System::IO::OVERLAPPED =
                unsafe { std::mem::zeroed() };
            let block_result = unsafe {
                LockFileEx(
                    handle,
                    LOCKFILE_EXCLUSIVE_LOCK,
                    0,
                    1,
                    0,
                    &mut blocking_overlapped,
                )
            };
            if block_result == 0 {
                let err = std::io::Error::last_os_error();
                return Err(anyhow::anyhow!("Failed to acquire download lock: {}", err));
            }
        }

        LockGuard {
            lock_path: lock_path.clone(),
            lock_file,
        }
    };

    // Check if already downloaded (AFTER acquiring lock to avoid TOCTOU race)
    if exe_path.exists() {
        info!("ChromeDriver already cached at: {:?}", exe_path);
        return Ok(exe_path);
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

    // Clean up zip file (lock file cleanup is handled by _lock_guard Drop)
    let _ = fs::remove_file(&zip_path);

    if exe_path.exists() {
        info!("ChromeDriver downloaded to: {:?}", exe_path);
        Ok(exe_path)
    } else {
        Err(anyhow::anyhow!(
            "Failed to extract chromedriver from downloaded zip"
        ))
    }
}

/// Download a driver based on browser type.
///
/// Routes to the appropriate driver download function based on the browser type.
fn download_driver_sync(
    browser_type: BrowserType,
    browser_version: Option<&str>,
) -> Result<PathBuf> {
    match browser_type {
        BrowserType::Chrome => download_chromedriver_sync(browser_version),
        BrowserType::Edge => download_edgedriver_sync(browser_version),
        BrowserType::Firefox => download_geckodriver_sync(),
        BrowserType::Safari => Err(anyhow::anyhow!(
            "SafariDriver is built into macOS and cannot be downloaded. \
            Please use Safari on macOS or choose a different browser."
        )),
    }
}

/// Download EdgeDriver synchronously.
fn download_edgedriver_sync(browser_version: Option<&str>) -> Result<PathBuf> {
    info!("Downloading EdgeDriver (this may take a while)...");

    let version_owned = browser_version.map(|s| s.to_string());

    // Check if we're already inside a Tokio runtime
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => match handle.runtime_flavor() {
            tokio::runtime::RuntimeFlavor::MultiThread => tokio::task::block_in_place(|| {
                handle.block_on(download_edgedriver_async(version_owned.as_deref()))
            }),
            tokio::runtime::RuntimeFlavor::CurrentThread => std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .with_context(|| "Failed to create runtime for driver download")?;
                rt.block_on(download_edgedriver_async(version_owned.as_deref()))
            })
            .join()
            .map_err(|_| {
                anyhow::anyhow!("EdgeDriver download failed: thread panicked during execution")
            })?,
            _ => std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .with_context(|| "Failed to create runtime for driver download")?;
                rt.block_on(download_edgedriver_async(version_owned.as_deref()))
            })
            .join()
            .map_err(|_| {
                anyhow::anyhow!("EdgeDriver download failed: thread panicked during execution")
            })?,
        },
        Err(_) => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .with_context(|| "Failed to create runtime for driver download")?;

            runtime.block_on(download_edgedriver_async(version_owned.as_deref()))
        }
    }
}

/// Download EdgeDriver asynchronously.
async fn download_edgedriver_async(browser_version: Option<&str>) -> Result<PathBuf> {
    let platform = get_platform_edge();
    let cache_dir = get_cache_dir()?;
    let client = reqwest::Client::builder()
        .user_agent("mcp-computer-use")
        .build()
        .with_context(|| "Failed to create HTTP client")?;

    // Edge uses the same version for both browser and driver
    let version = if let Some(ver) = browser_version {
        ver.to_string()
    } else {
        // Get latest stable Edge version
        get_latest_edge_version(&client).await?
    };

    info!("Downloading EdgeDriver {} for {}...", version, platform);

    // Construct download URL
    let download_url = build_msedgedriver_download_url(&version, platform)?;

    // Create version-specific directory
    let version_dir = cache_dir.join(format!("msedgedriver-{}", version));
    if !version_dir.exists() {
        fs::create_dir_all(&version_dir)?;
    }

    let exe_name = get_msedgedriver_exe_name();
    let exe_path = version_dir.join(exe_name);

    // Check if already downloaded
    if exe_path.exists() {
        info!("EdgeDriver already cached at: {:?}", exe_path);
        return Ok(exe_path);
    }

    // Download the zip file
    let zip_response = client
        .get(&download_url)
        .send()
        .await
        .with_context(|| format!("Failed to download EdgeDriver from {}", download_url))?;

    if !zip_response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Failed to download EdgeDriver: HTTP {}",
            zip_response.status()
        ));
    }

    let zip_bytes = zip_response
        .bytes()
        .await
        .with_context(|| "Failed to read EdgeDriver download")?;

    // Save zip to temp file
    let zip_path = version_dir.join("msedgedriver.zip");
    let mut zip_file = fs::File::create(&zip_path)?;
    zip_file.write_all(&zip_bytes)?;
    drop(zip_file);

    // Extract the zip
    let zip_file = fs::File::open(&zip_path)?;
    let mut archive =
        zip::ZipArchive::new(zip_file).with_context(|| "Failed to open EdgeDriver zip")?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let file_name = file.name().to_string();

        // Find the msedgedriver executable
        let is_msedgedriver = if let Some(basename) = file_name.split('/').next_back() {
            basename == exe_name && !file.is_dir()
        } else {
            false
        };

        if is_msedgedriver {
            let mut exe_file = fs::File::create(&exe_path)?;
            std::io::copy(&mut file, &mut exe_file)?;

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

    // Clean up zip file
    let _ = fs::remove_file(&zip_path);

    if exe_path.exists() {
        info!("EdgeDriver downloaded to: {:?}", exe_path);
        Ok(exe_path)
    } else {
        Err(anyhow::anyhow!(
            "Failed to extract msedgedriver from downloaded zip"
        ))
    }
}

/// Get the latest stable Edge version.
async fn get_latest_edge_version(client: &reqwest::Client) -> Result<String> {
    // Microsoft provides a LATEST_STABLE file with the version
    let url = format!("{}/LATEST_STABLE", MSEDGEDRIVER_BASE_URL);
    let response = client
        .get(&url)
        .send()
        .await
        .with_context(|| "Failed to fetch latest Edge version")?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Failed to get latest Edge version: HTTP {}",
            response.status()
        ));
    }

    let version = response
        .text()
        .await
        .with_context(|| "Failed to read Edge version response")?
        .trim()
        .to_string();

    debug!("Latest Edge version: {}", version);
    Ok(version)
}

/// Download GeckoDriver synchronously.
fn download_geckodriver_sync() -> Result<PathBuf> {
    info!("Downloading GeckoDriver (this may take a while)...");

    // Check if we're already inside a Tokio runtime
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => match handle.runtime_flavor() {
            tokio::runtime::RuntimeFlavor::MultiThread => {
                tokio::task::block_in_place(|| handle.block_on(download_geckodriver_async()))
            }
            tokio::runtime::RuntimeFlavor::CurrentThread => std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .with_context(|| "Failed to create runtime for driver download")?;
                rt.block_on(download_geckodriver_async())
            })
            .join()
            .map_err(|_| {
                anyhow::anyhow!("GeckoDriver download failed: thread panicked during execution")
            })?,
            _ => std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .with_context(|| "Failed to create runtime for driver download")?;
                rt.block_on(download_geckodriver_async())
            })
            .join()
            .map_err(|_| {
                anyhow::anyhow!("GeckoDriver download failed: thread panicked during execution")
            })?,
        },
        Err(_) => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .with_context(|| "Failed to create runtime for driver download")?;

            runtime.block_on(download_geckodriver_async())
        }
    }
}

/// Download GeckoDriver asynchronously.
async fn download_geckodriver_async() -> Result<PathBuf> {
    let platform = get_platform_geckodriver();
    let cache_dir = get_cache_dir()?;
    let client = reqwest::Client::builder()
        .user_agent("mcp-computer-use")
        .build()
        .with_context(|| "Failed to create HTTP client")?;

    // Get latest release info from GitHub API
    let response: serde_json::Value = client
        .get(GECKODRIVER_RELEASES_URL)
        .send()
        .await
        .with_context(|| "Failed to fetch GeckoDriver releases")?
        .json()
        .await
        .with_context(|| "Failed to parse GeckoDriver releases JSON")?;

    let version = response
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Could not find GeckoDriver version in release info"))?;

    info!("Downloading GeckoDriver {} for {}...", version, platform);

    // Find the appropriate asset for our platform
    let assets = response
        .get("assets")
        .and_then(|a| a.as_array())
        .ok_or_else(|| anyhow::anyhow!("Could not find assets in GeckoDriver release"))?;

    let expected_name = build_geckodriver_archive_name(version, platform)?;

    let download_url = if let Some(url) = assets
        .iter()
        .find(|asset| {
            asset
                .get("name")
                .and_then(|n| n.as_str())
                .is_some_and(|name| name == expected_name)
        })
        .and_then(|asset| asset.get("browser_download_url"))
        .and_then(|u| u.as_str())
    {
        url.to_string()
    } else {
        if expected_name.contains(['/', '\\']) {
            return Err(anyhow::anyhow!(
                "Invalid GeckoDriver asset name: {}",
                expected_name
            ));
        }
        let fallback = format!("{}/{}", GECKODRIVER_LATEST_DOWNLOAD_BASE_URL, expected_name);
        warn!(
            "GeckoDriver asset '{}' not found; falling back to {}",
            expected_name, fallback
        );
        fallback
    };

    // Create version-specific directory
    let version_dir = cache_dir.join(format!("geckodriver-{}", version));
    if !version_dir.exists() {
        fs::create_dir_all(&version_dir)?;
    }

    let exe_name = get_geckodriver_exe_name();
    let exe_path = version_dir.join(exe_name);

    // Check if already downloaded
    if exe_path.exists() {
        info!("GeckoDriver already cached at: {:?}", exe_path);
        return Ok(exe_path);
    }

    // Download the archive
    let archive_response = client
        .get(&download_url)
        .send()
        .await
        .with_context(|| "Failed to download GeckoDriver")?;

    let archive_bytes = archive_response
        .bytes()
        .await
        .with_context(|| "Failed to read GeckoDriver download")?;

    if cfg!(target_os = "windows") {
        // Extract from zip on Windows
        let archive_path = version_dir.join("geckodriver.zip");
        let mut archive_file = fs::File::create(&archive_path)?;
        archive_file.write_all(&archive_bytes)?;
        drop(archive_file);

        let archive_file = fs::File::open(&archive_path)?;
        let mut archive =
            zip::ZipArchive::new(archive_file).with_context(|| "Failed to open GeckoDriver zip")?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let file_name = file.name().to_string();

            if file_name.ends_with(exe_name) && !file.is_dir() {
                let mut exe_file = fs::File::create(&exe_path)?;
                std::io::copy(&mut file, &mut exe_file)?;
                break;
            }
        }

        if !exe_path.exists() {
            return Err(anyhow::anyhow!(
                "Failed to find GeckoDriver executable '{}' in downloaded zip",
                exe_name
            ));
        }

        let _ = fs::remove_file(&archive_path);
    } else {
        // Extract from tar.gz on Unix
        use std::io::Read;

        let archive_path = version_dir.join("geckodriver.tar.gz");
        let mut archive_file = fs::File::create(&archive_path)?;
        archive_file.write_all(&archive_bytes)?;
        drop(archive_file);

        // Use Command to extract tar.gz
        let archive_path_str = archive_path.to_string_lossy().into_owned();
        let version_dir_str = version_dir.to_string_lossy().into_owned();
        let output = Command::new("tar")
            .args(["-xzf", &archive_path_str, "-C", &version_dir_str])
            .output()
            .with_context(|| "Failed to extract GeckoDriver tar.gz")?;

        if !output.status.success() {
            // Fallback: try manual extraction if tar command fails
            let archive_file = fs::File::open(&archive_path)?;
            let gz = flate2::read::GzDecoder::new(archive_file);
            let mut archive = tar::Archive::new(gz);

            for entry in archive.entries()? {
                let mut entry = entry?;
                let path = entry.path()?;
                if path.file_name().is_some_and(|n| n == exe_name) {
                    let mut exe_file = fs::File::create(&exe_path)?;
                    let mut contents = Vec::new();
                    entry.read_to_end(&mut contents)?;
                    exe_file.write_all(&contents)?;

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
        } else {
            // Make executable
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if exe_path.exists() {
                    let mut perms = fs::metadata(&exe_path)?.permissions();
                    perms.set_mode(0o755);
                    fs::set_permissions(&exe_path, perms)?;
                }
            }
        }

        let _ = fs::remove_file(&archive_path);
    }

    if exe_path.exists() {
        info!("GeckoDriver downloaded to: {:?}", exe_path);
        Ok(exe_path)
    } else {
        Err(anyhow::anyhow!(
            "Failed to extract geckodriver from downloaded archive"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_manager_creation() {
        let manager = DriverManager::new();
        assert!(manager.driver_process.is_none());
        assert!(manager.driver_path.is_none());
        assert_eq!(manager.port, crate::config::DEFAULT_DRIVER_PORT);
    }

    #[test]
    fn test_get_platform_chrome() {
        let platform = get_platform_chrome();
        // Platform should be a non-empty string
        assert!(!platform.is_empty());
        // Platform should be one of the known values
        let valid_platforms = ["win64", "win32", "mac-arm64", "mac-x64", "linux64"];
        assert!(
            valid_platforms.contains(&platform),
            "Platform '{}' should be one of: {:?}",
            platform,
            valid_platforms
        );
    }

    #[test]
    fn test_get_platform_edge() {
        let platform = get_platform_edge();
        assert!(!platform.is_empty());
        let valid_platforms = ["win64", "win32", "arm64", "mac64", "mac64_m1", "linux64"];
        assert!(
            valid_platforms.contains(&platform),
            "Platform '{}' should be one of: {:?}",
            platform,
            valid_platforms
        );
    }

    #[test]
    fn test_get_platform_geckodriver() {
        let platform = get_platform_geckodriver();
        assert!(!platform.is_empty());
        let valid_platforms = [
            "win64",
            "win32",
            "win-aarch64",
            "macos",
            "macos-aarch64",
            "linux64",
            "linux32",
            "linux-aarch64",
        ];
        assert!(
            valid_platforms.contains(&platform),
            "Platform '{}' should be one of: {:?}",
            platform,
            valid_platforms
        );
    }

    #[test]
    fn test_get_chromedriver_exe_name() {
        let exe_name = get_chromedriver_exe_name();
        #[cfg(target_os = "windows")]
        assert_eq!(exe_name, "chromedriver.exe");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(exe_name, "chromedriver");
    }

    #[test]
    fn test_get_msedgedriver_exe_name() {
        let exe_name = get_msedgedriver_exe_name();
        #[cfg(target_os = "windows")]
        assert_eq!(exe_name, "msedgedriver.exe");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(exe_name, "msedgedriver");
    }

    #[test]
    fn test_get_geckodriver_exe_name() {
        let exe_name = get_geckodriver_exe_name();
        #[cfg(target_os = "windows")]
        assert_eq!(exe_name, "geckodriver.exe");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(exe_name, "geckodriver");
    }

    #[test]
    fn test_build_msedgedriver_download_url() {
        let url = build_msedgedriver_download_url("1.2.3", "win64").unwrap();
        assert_eq!(
            url,
            format!("{}/1.2.3/edgedriver_win64.zip", MSEDGEDRIVER_BASE_URL)
        );
    }

    #[test]
    fn test_build_geckodriver_archive_name() {
        let name = build_geckodriver_archive_name("v0.34.0", "linux64").unwrap();
        let expected = "geckodriver-v0.34.0-linux64.tar.gz";
        assert_eq!(name, expected);
    }

    #[test]
    fn test_invalid_driver_component_rejected() {
        assert!(build_msedgedriver_download_url("../bad", "win64").is_err());
        assert!(build_geckodriver_archive_name("v0.34.0", "bad/plat").is_err());
    }

    #[test]
    fn test_runtime_detection_outside_runtime() {
        // When called outside a runtime, try_current should return Err
        // This test verifies the fallback path is correctly triggered
        let result = tokio::runtime::Handle::try_current();
        assert!(
            result.is_err(),
            "Should not be inside a runtime in regular test"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_runtime_detection_inside_multi_thread_runtime() {
        // When called inside a multi-threaded runtime, try_current should return Ok
        let result = tokio::runtime::Handle::try_current();
        assert!(
            result.is_ok(),
            "Should detect runtime when inside async context"
        );

        let handle = result.unwrap();
        // This test uses multi_thread flavor explicitly
        assert_eq!(
            handle.runtime_flavor(),
            tokio::runtime::RuntimeFlavor::MultiThread,
            "Should detect multi-threaded runtime"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_runtime_detection_inside_current_thread_runtime() {
        // When called inside a current_thread runtime, try_current should return Ok
        let result = tokio::runtime::Handle::try_current();
        assert!(
            result.is_ok(),
            "Should detect runtime when inside async context"
        );

        let handle = result.unwrap();
        assert_eq!(
            handle.runtime_flavor(),
            tokio::runtime::RuntimeFlavor::CurrentThread,
            "Should detect current_thread runtime"
        );
    }
}

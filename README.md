# MCP Computer Use

A Rust MCP (Model Context Protocol) server that provides browser control capabilities for AI models. This implementation uses the `thirtyfour` WebDriver library to control browsers and implements all Gemini computer use predefined tools.

## Features

- **Full Browser Control**: Click, hover, type, scroll, navigate, and more
- **Tab Management**: Create, close, switch between, and list browser tabs
- **Screenshot Capture**: Every action returns a screenshot for visual feedback
- **Configurable**: Set browser binary path, WebDriver URL, screen size, and more
- **Tool Filtering**: Disable specific tools as needed
- **Multiple Transports**: Supports both stdio and HTTP streamable transports
- **Auto-Start Mode**: Automatically launches ChromeDriver and browser
- **Auto-Download Driver**: Automatically downloads ChromeDriver if not found
- **CDP Mode**: Chrome DevTools Protocol connection for existing browser control
- **Undetected Mode**: Stealth mode to help avoid bot detection (inspired by patchright)
- **Smart Detection**: Auto-detect browser and driver from PATH and common locations

## Prerequisites

- Rust 1.70 or later
- Chrome browser installed (auto-detected from PATH or common locations)
- One of the following:
  - Use `MCP_AUTO_START=true` for fully automatic setup (recommended)
  - A WebDriver server running (e.g., ChromeDriver)
  - Use CDP mode (`MCP_CONNECTION_MODE=cdp`) with an existing browser

## Quick Start

The easiest way to get started is with auto-start mode:

```bash
# Build the project
cargo build --release

# Run with automatic driver and browser management
MCP_AUTO_START=true MCP_AUTO_DOWNLOAD_DRIVER=true ./target/release/mcp-computer-use
```

This will:
1. Auto-detect Chrome browser on your system
2. Download ChromeDriver if not found
3. Launch ChromeDriver automatically
4. Start the MCP server

> ⚠️ **Security Note**: The `MCP_AUTO_DOWNLOAD_DRIVER=true` option downloads ChromeDriver from Google's Chrome for Testing API at runtime. While this is convenient for development, for production environments consider:
> - Pre-installing ChromeDriver from trusted sources
> - Using `MCP_DRIVER_PATH` to point to a verified driver binary
> - Auditing the downloaded binary before use

## Installation

```bash
# Clone the repository
git clone https://github.com/hugefiver/mcp-computer-use.git
cd mcp-computer-use

# Build the project
cargo build --release

# The binary will be at target/release/mcp-computer-use
```

## Configuration

The server can be configured using environment variables:

### Core Settings

| Variable | Description | Default |
|----------|-------------|---------|
| `MCP_AUTO_START` | Automatically manage browser/driver lifecycle | `false` |
| `MCP_AUTO_DOWNLOAD_DRIVER` | Download ChromeDriver if not found | `false` |
| `MCP_CONNECTION_MODE` | Connection mode: `webdriver` or `cdp` | `webdriver` |
| `MCP_HEADLESS` | Run browser in headless mode | `true` |

### Browser Settings

| Variable | Description | Default |
|----------|-------------|---------|
| `MCP_BROWSER_PATH` | Path to the browser binary | (auto-detect) |
| `MCP_BROWSER_TYPE` | Browser type (currently only `chrome`) | `chrome` |
| `MCP_SCREEN_WIDTH` | Screen width in pixels | `1280` |
| `MCP_SCREEN_HEIGHT` | Screen height in pixels | `720` |
| `MCP_INITIAL_URL` | Initial URL to load | `https://www.google.com` |
| `MCP_SEARCH_ENGINE_URL` | Search engine URL for search action | `https://www.google.com` |
| `MCP_UNDETECTED` | Enable undetected/stealth mode | `false` |

### Driver Settings

| Variable | Description | Default |
|----------|-------------|---------|
| `MCP_DRIVER_PATH` | Path to browser driver executable | (auto-detect) |
| `MCP_DRIVER_PORT` | Port for driver | `9515` |

### WebDriver Settings

| Variable | Description | Default |
|----------|-------------|---------|
| `MCP_WEBDRIVER_URL` | WebDriver server URL | `http://localhost:9515` |
| `MCP_CDP_PORT` | CDP port for browser connection | `9222` |

### Transport Settings

| Variable | Description | Default |
|----------|-------------|---------|
| `MCP_TRANSPORT` | Transport mode: `stdio` or `http` | `stdio` |
| `MCP_HTTP_HOST` | HTTP server host | `127.0.0.1` |
| `MCP_HTTP_PORT` | HTTP server port | `8080` |

### Other Settings

| Variable | Description | Default |
|----------|-------------|---------|
| `MCP_DISABLED_TOOLS` | Comma-separated list of tools to disable | (empty) |

## Usage Modes

### 1. Auto-Start Mode (Recommended)

The simplest way to use this server. Automatically manages ChromeDriver and browser:

```bash
MCP_AUTO_START=true \
MCP_AUTO_DOWNLOAD_DRIVER=true \
./target/release/mcp-computer-use
```

### 2. Manual WebDriver Mode

If you want to manage ChromeDriver yourself:

```bash
# Start ChromeDriver manually
chromedriver --port=9515 &

# Run the MCP server
MCP_WEBDRIVER_URL=http://localhost:9515 \
./target/release/mcp-computer-use
```

### 3. CDP Mode (Connect to Existing Browser)

Connect to an already running Chrome browser with debugging enabled:

```bash
# Start Chrome with debugging enabled
google-chrome --remote-debugging-port=9222

# Start ChromeDriver separately
chromedriver --port=9515 &

# In another terminal, run the MCP server (connects to existing browser via ChromeDriver)
# Note: MCP_WEBDRIVER_URL points to your running ChromeDriver
MCP_CONNECTION_MODE=cdp \
MCP_WEBDRIVER_URL=http://localhost:9515 \
./target/release/mcp-computer-use
```

Or let the server manage everything automatically:

```bash
MCP_CONNECTION_MODE=cdp \
MCP_AUTO_START=true \
./target/release/mcp-computer-use
```

### 4. HTTP Transport Mode

Run the server with HTTP streamable transport:

```bash
MCP_TRANSPORT=http \
MCP_AUTO_START=true \
./target/release/mcp-computer-use
```

The HTTP server exposes an MCP endpoint at `/mcp`.

> **Security note:** The HTTP endpoint does not provide authentication or encryption. Only bind to localhost unless you have proper security measures in place.

### Undetected Mode

Enable stealth mode to help avoid bot detection:

```bash
MCP_UNDETECTED=true \
MCP_AUTO_START=true \
./target/release/mcp-computer-use
```

This applies various anti-detection techniques inspired by [patchright](https://github.com/Kaliiiiiiiiii-Vinyzu/patchright).

## Available Tools

The server implements all Gemini computer use predefined tools plus additional tab management tools:

| Tool | Description |
|------|-------------|
| `open_web_browser` | Opens the web browser. Call this first before any other actions. |
| `click_at` | Clicks at a specific x, y coordinate on the webpage. |
| `hover_at` | Hovers at a specific x, y coordinate (for dropdown menus, etc.). |
| `type_text_at` | Types text at a specific x, y coordinate. |
| `scroll_document` | Scrolls the entire webpage in the specified direction. |
| `scroll_at` | Scrolls at a specific coordinate with specified magnitude. |
| `wait_5_seconds` | Waits 5 seconds for page processes to complete. |
| `go_back` | Navigates back in browser history. |
| `go_forward` | Navigates forward in browser history. |
| `search` | Navigates to the search engine home page. |
| `navigate` | Navigates directly to a specified URL. |
| `key_combination` | Presses keyboard keys and combinations. |
| `drag_and_drop` | Drags an element from one position to another. |
| `current_state` | Returns the current screenshot and URL. |
| `new_tab` | Creates a new browser tab, optionally navigating to a URL. |
| `close_tab` | Closes a browser tab by handle (or current tab if not specified). |
| `switch_tab` | Switches to a different tab by handle or index. |
| `list_tabs` | Lists all open browser tabs with their handles, URLs, and titles. |

### Disabling Tools

```bash
MCP_DISABLED_TOOLS=drag_and_drop,key_combination ./target/release/mcp-computer-use
```

## MCP Client Integration

### Claude Desktop Configuration

Add to your Claude Desktop configuration (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "browser": {
      "command": "/path/to/mcp-computer-use",
      "env": {
        "MCP_AUTO_START": "true",
        "MCP_AUTO_DOWNLOAD_DRIVER": "true",
        "MCP_HEADLESS": "false"
      }
    }
  }
}
```

### Programmatic Usage

First call `open_web_browser` to start the browser, then use other tools to interact with web pages. Each tool returns a JSON response with the current URL and a base64-encoded screenshot.

## Architecture

```
mcp-computer-use/
├── src/
│   ├── main.rs           # Entry point and MCP server setup
│   ├── config.rs         # Configuration management
│   ├── browser.rs        # Browser controller using thirtyfour
│   ├── browser_manager.rs # Browser detection and CDP launch
│   ├── driver.rs         # WebDriver management and auto-download
│   └── tools.rs          # MCP tool definitions
├── Cargo.toml            # Dependencies and project metadata
└── README.md             # This file
```

## Development

```bash
# Run with debug logging
RUST_LOG=debug cargo run

# Run tests
cargo test

# Format code
cargo fmt

# Lint code
cargo clippy
```

## CI/CD

This project uses GitHub Actions for continuous integration and deployment:

### Workflows

| Workflow | Trigger | Description |
|----------|---------|-------------|
| **CI** | Push/PR to any branch | Runs lint (`fmt`, `clippy`), tests, and builds. |
| **Prerelease** | Push to `main`/`master`/`dev` | Builds for multiple platforms and creates a prerelease. |
| **Release** | Tag push (`v*`) | Builds for multiple platforms and creates a release. |

### Supported Platforms

- Linux x64 (`x86_64-unknown-linux-gnu`)
- macOS x64 (`x86_64-apple-darwin`)
- macOS ARM64 (`aarch64-apple-darwin`)
- Windows x64 (`x86_64-pc-windows-msvc`)

### Creating a Release

```bash
git tag v1.0.0
git push origin v1.0.0
```

## References

- [Gemini Computer Use Documentation](https://ai.google.dev/gemini-api/docs/computer-use)
- [Google Gemini Computer Use Preview](https://github.com/google-gemini/computer-use-preview)
- [Model Context Protocol](https://modelcontextprotocol.io/)
- [thirtyfour WebDriver](https://github.com/stevepryde/thirtyfour)
- [rmcp - Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk)

## License

MIT

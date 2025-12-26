# MCP Computer Use

A Rust MCP (Model Context Protocol) server that provides browser control capabilities for AI models. This implementation uses the `thirtyfour` WebDriver library to control browsers and implements all Gemini computer use predefined tools.

## Features

- **Full Browser Control**: Click, hover, type, scroll, navigate, and more
- **Tab Management**: Create, close, switch between, and list browser tabs
- **Screenshot Capture**: Every action returns a screenshot for visual feedback
- **Configurable**: Set browser binary path, WebDriver URL, screen size, and more
- **Tool Filtering**: Disable specific tools as needed
- **Cross-Browser Support**: Works with Chrome, Firefox, Edge, and Safari
- **Multiple Transports**: Supports both stdio and HTTP streamable transports
- **Auto-Launch Driver**: Optionally auto-start ChromeDriver
- **Undetected Mode**: Stealth mode to help avoid bot detection (inspired by patchright)

## Prerequisites

- Rust 1.70 or later
- A WebDriver server running (e.g., ChromeDriver, GeckoDriver) or use auto-launch feature
- The corresponding browser installed

### Installing ChromeDriver

```bash
# On Ubuntu/Debian
sudo apt install chromium-chromedriver

# On macOS with Homebrew
brew install chromedriver

# Or download from https://chromedriver.chromium.org/downloads
```

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

| Variable | Description | Default |
|----------|-------------|---------|
| `MCP_BROWSER_BINARY_PATH` | Path to the browser binary | System default |
| `MCP_WEBDRIVER_URL` | WebDriver server URL | `http://localhost:9515` |
| `MCP_BROWSER_TYPE` | Browser type: chrome, firefox, edge, safari | `chrome` |
| `MCP_SCREEN_WIDTH` | Screen width in pixels | `1280` |
| `MCP_SCREEN_HEIGHT` | Screen height in pixels | `720` |
| `MCP_INITIAL_URL` | Initial URL to load | `https://www.google.com` |
| `MCP_SEARCH_ENGINE_URL` | Search engine URL for search action | `https://www.google.com` |
| `MCP_HEADLESS` | Run browser in headless mode | `true` |
| `MCP_DISABLED_TOOLS` | Comma-separated list of tools to disable | (empty) |
| `MCP_HIGHLIGHT_MOUSE` | Highlight mouse position for debugging | `false` |
| `MCP_TRANSPORT` | Transport mode: `stdio` or `http` | `stdio` |
| `MCP_HTTP_HOST` | HTTP server host (when using http transport) | `127.0.0.1` |
| `MCP_HTTP_PORT` | HTTP server port (when using http transport) | `8080` |
| `MCP_AUTO_LAUNCH_DRIVER` | Automatically launch browser driver | `false` |
| `MCP_DRIVER_PATH` | Path to browser driver executable | (auto-detect) |
| `MCP_DRIVER_PORT` | Port for auto-launched driver | `9515` |
| `MCP_UNDETECTED` | Enable undetected/stealth mode | `false` |

### Example Configuration

```bash
# Start ChromeDriver first (or use MCP_AUTO_LAUNCH_DRIVER=true)
chromedriver --port=9515 &

# Run the MCP server with custom configuration
MCP_BROWSER_TYPE=chrome \
MCP_HEADLESS=true \
MCP_SCREEN_WIDTH=1920 \
MCP_SCREEN_HEIGHT=1080 \
./target/release/mcp-computer-use
```

### HTTP Transport Mode

To run the server with HTTP streamable transport instead of stdio:

```bash
MCP_TRANSPORT=http \
MCP_HTTP_HOST=127.0.0.1 \
MCP_HTTP_PORT=8080 \
./target/release/mcp-computer-use
```

The HTTP server exposes an MCP endpoint at `/mcp`.

> **Security note:** The HTTP endpoint does not provide authentication or encryption by default. Do not expose it directly to untrusted networks or the public internet. Bind it only to localhost (for example, `127.0.0.1`) unless you place it behind appropriate security protections such as TLS termination, authentication, and firewall rules.
### Auto-Launch Driver

To automatically launch ChromeDriver:

```bash
MCP_AUTO_LAUNCH_DRIVER=true \
./target/release/mcp-computer-use
```

### Undetected Mode

To enable stealth/undetected mode that helps avoid bot detection:

```bash
MCP_UNDETECTED=true \
./target/release/mcp-computer-use
```

This mode applies various anti-detection techniques inspired by [patchright](https://github.com/Kaliiiiiiiiii-Vinyzu/patchright).

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

To disable specific tools, set the `MCP_DISABLED_TOOLS` environment variable:

```bash
# Disable drag_and_drop and key_combination tools
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
        "MCP_BROWSER_TYPE": "chrome",
        "MCP_WEBDRIVER_URL": "http://localhost:9515",
        "MCP_HEADLESS": "false"
      }
    }
  }
}
```

### Programmatic Usage

When using with an MCP client, first call `open_web_browser` to start the browser, then use other tools to interact with web pages. Each tool returns a JSON response with the current URL and a base64-encoded screenshot.

## Architecture

```
mcp-computer-use/
├── src/
│   ├── main.rs      # Entry point and MCP server setup
│   ├── config.rs    # Configuration management
│   ├── browser.rs   # Browser controller using thirtyfour
│   └── tools.rs     # MCP tool definitions
├── Cargo.toml       # Dependencies and project metadata
└── README.md        # This file
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
| **CI** | Push/PR to any branch | Runs lint (fmt, clippy), tests, and builds. Uploads build artifacts. |
| **Prerelease** | Push to `main`/`master`/`dev` | Builds for multiple platforms and creates a prerelease. |
| **Release** | Tag push (`v*`) or release publish | Builds for multiple platforms and creates/updates the release with assets. |

### Supported Platforms

Release binaries are built for the following platforms:
- Linux x64 (`x86_64-unknown-linux-gnu`)
- macOS x64 (`x86_64-apple-darwin`)
- macOS ARM64 (`aarch64-apple-darwin`)
- Windows x64 (`x86_64-pc-windows-msvc`)

### Creating a Release

1. Tag a commit: `git tag v1.0.0`
2. Push the tag: `git push origin v1.0.0`
3. The release workflow will automatically build and create a GitHub release with binaries for all platforms.

## References

- [Gemini Computer Use Documentation](https://ai.google.dev/gemini-api/docs/computer-use)
- [Google Gemini Computer Use Preview](https://github.com/google-gemini/computer-use-preview)
- [Model Context Protocol](https://modelcontextprotocol.io/)
- [thirtyfour WebDriver](https://github.com/stevepryde/thirtyfour)
- [rmcp - Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk)

## License

MIT
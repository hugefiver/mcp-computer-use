# MCP Computer Use

[![CI](https://github.com/hugefiver/mcp-computer-use/actions/workflows/ci.yml/badge.svg)](https://github.com/hugefiver/mcp-computer-use/actions/workflows/ci.yml) [![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

[English](README.md) | [简体中文](README.zh-CN.md) | 繁體中文 | [Esperanto](README.eo.md)

## 簡介

一個以 Rust 撰寫的 MCP（Model Context Protocol）伺服器，為 AI 模型提供瀏覽器控制能力。基於 `thirtyfour` WebDriver 函式庫，支援所有 Gemini 預定義的電腦使用工具。

## 功能亮點

- **完整瀏覽器控制**：點擊、懸停、輸入、捲動、導航等
- **分頁管理**：建立、關閉、切換、列出分頁
- **每次操作皆回傳截圖**，便於視覺化回饋
- **多種運行模式**：支援 stdio 與 HTTP 串流傳輸
- **自動化**：可自動啟動瀏覽器與驅動，也支援手動/既有工作階段

## 快速開始

```bash
# 建置專案
cargo build --release

# 預設使用 Chrome，並自動啟動與下載驅動
MCP_AUTO_START=true MCP_AUTO_DOWNLOAD_DRIVER=true ./target/release/mcp-computer-use

# 使用 Edge 或 Firefox
MCP_BROWSER_TYPE=edge MCP_AUTO_START=true MCP_AUTO_DOWNLOAD_DRIVER=true ./target/release/mcp-computer-use
MCP_BROWSER_TYPE=firefox MCP_AUTO_START=true MCP_AUTO_DOWNLOAD_DRIVER=true ./target/release/mcp-computer-use
```

## 設定速覽

| 變數 | 說明 | 預設值 |
| ---- | ---- | ------ |
| `MCP_AUTO_START` | 是否自動管理瀏覽器/驅動生命週期 | `false` |
| `MCP_AUTO_DOWNLOAD_DRIVER` | 找不到時自動下載兼容的瀏覽器驅動 | `false` |
| `MCP_CONNECTION_MODE` | 連線模式：`webdriver` 或 `cdp` | `webdriver` |
| `MCP_BROWSER_TYPE` | 瀏覽器類型：`chrome`、`edge`、`firefox`、`safari` | `chrome` |
| `MCP_HEADLESS` | 是否以無頭模式執行瀏覽器 | `true` |
| `MCP_OPEN_BROWSER_ON_START` | MCP 啟動時是否預先開啟瀏覽器 | `false` |
| `MCP_TRANSPORT` | 傳輸方式：`stdio` 或 `http` | `stdio` |

更多環境變數與使用方式請參考 [英文 README](README.md)。

## 開發

```bash
cargo test
cargo fmt
cargo clippy
```

## 授權

MIT

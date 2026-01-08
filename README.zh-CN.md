# MCP Computer Use

[![CI](https://github.com/hugefiver/mcp-computer-use/actions/workflows/ci.yml/badge.svg)](https://github.com/hugefiver/mcp-computer-use/actions/workflows/ci.yml) [![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

[English](README.md) | 简体中文 | [繁體中文](README.zh-TW.md) | [Esperanto](README.eo.md)

## 简介

一个使用 Rust 编写的 MCP（Model Context Protocol）服务器，为 AI 模型提供浏览器控制能力。基于 `thirtyfour` WebDriver 库，支持所有 Gemini 预定义的电脑使用工具。

## 功能亮点

- **完整浏览器控制**：点击、悬停、输入、滚动、导航等
- **标签管理**：创建、关闭、切换、列出标签页
- **每次操作返回截图**，便于可视化反馈
- **多种运行模式**：支持 stdio 与 HTTP 流式传输
- **自动化**：可自动启动浏览器与驱动，也支持手动/现有会话

## 快速开始

```bash
# 构建项目
cargo build --release

# 默认使用 Chrome，自动启动并下载驱动
MCP_AUTO_START=true MCP_AUTO_DOWNLOAD_DRIVER=true ./target/release/mcp-computer-use

# 使用 Edge 或 Firefox
MCP_BROWSER_TYPE=edge MCP_AUTO_START=true MCP_AUTO_DOWNLOAD_DRIVER=true ./target/release/mcp-computer-use
MCP_BROWSER_TYPE=firefox MCP_AUTO_START=true MCP_AUTO_DOWNLOAD_DRIVER=true ./target/release/mcp-computer-use
```

## 配置速览

| 变量 | 说明 | 默认值 |
| ---- | ---- | ------ |
| `MCP_AUTO_START` | 是否自动管理浏览器/驱动生命周期 | `false` |
| `MCP_AUTO_DOWNLOAD_DRIVER` | 未找到时自动下载匹配的浏览器驱动 | `false` |
| `MCP_CONNECTION_MODE` | 连接模式：`webdriver` 或 `cdp` | `webdriver` |
| `MCP_BROWSER_TYPE` | 浏览器类型：`chrome`、`edge`、`firefox`、`safari` | `chrome` |
| `MCP_HEADLESS` | 是否以无头模式运行浏览器 | `true` |
| `MCP_OPEN_BROWSER_ON_START` | MCP 启动时是否预先打开浏览器 | `false` |
| `MCP_TRANSPORT` | 传输方式：`stdio` 或 `http` | `stdio` |

更多环境变量和使用方式请参见 [英文 README](README.md)。

## 开发

```bash
cargo test
cargo fmt
cargo clippy
```

## 许可证

MIT

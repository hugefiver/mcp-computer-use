# MCP Computer Use

[![CI](https://github.com/hugefiver/mcp-computer-use/actions/workflows/ci.yml/badge.svg)](https://github.com/hugefiver/mcp-computer-use/actions/workflows/ci.yml) [![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

[English](README.md) | [简体中文](README.zh-CN.md) | [繁體中文](README.zh-TW.md) | Esperanto

## Enkonduko

Rust-a MCP (Model Context Protocol) servilo kiu provizas foliumilan regadon por AI-modeloj. Ĝi uzas la bibliotekon `thirtyfour` por WebDriver kaj kovras ĉiujn antaŭdifinitajn ilojn de Gemini computer use.

## Ecoj

- **Plena rego de la retumilo**: klaki, ŝvebi, tajpi, rulumadi, navigi, ktp.
- **Langeta administrado**: krei, fermi, ŝalti, listigi langetojn
- **Ekrankopio post ĉiu ago** por klara vida reagado
- **Multoblaj transportoj**: subteno por stdio kaj HTTP flua reĝimo
- **Aŭtomata prizorgado**: laŭvola aŭto-komenco kaj aŭto-elŝuto de stiriloj

## Rapida starto

```bash
# Konstrui
cargo build --release

# Defaŭlte uzu Chrome, aŭtomate lanĉu kaj elŝutu la stirilon
MCP_AUTO_START=true MCP_AUTO_DOWNLOAD_DRIVER=true ./target/release/mcp-computer-use

# Uzu Edge aŭ Firefox
MCP_BROWSER_TYPE=edge MCP_AUTO_START=true MCP_AUTO_DOWNLOAD_DRIVER=true ./target/release/mcp-computer-use
MCP_BROWSER_TYPE=firefox MCP_AUTO_START=true MCP_AUTO_DOWNLOAD_DRIVER=true ./target/release/mcp-computer-use
```

## Agorda superrigardo

| Variablo | Priskribo | Defaŭlta valoro |
| -------- | --------- | --------------- |
| `MCP_AUTO_START` | Ĉu aŭtomate mastrumi la retumilon/stirilon | `false` |
| `MCP_AUTO_DOWNLOAD_DRIVER` | Elŝuti kongruan stirilon se ne trovita | `false` |
| `MCP_CONNECTION_MODE` | Konekta reĝimo: `webdriver` aŭ `cdp` | `webdriver` |
| `MCP_BROWSER_TYPE` | Retumila tipo: `chrome`, `edge`, `firefox`, `safari` | `chrome` |
| `MCP_HEADLESS` | Ĉu ruli la retumilon sen kapo | `true` |
| `MCP_OPEN_BROWSER_ON_START` | Ĉu malfermi retumilon tuj post starto | `false` |
| `MCP_TRANSPORT` | Transporto: `stdio` aŭ `http` | `stdio` |

Pli da detaloj troviĝas en la [angla README](README.md).

## Disvolvado

```bash
cargo test
cargo fmt
cargo clippy
```

## Permesilo

MIT

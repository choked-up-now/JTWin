# 汲湍 (JiTuan)

[![Version](https://img.shields.io/badge/version-0.7.2-blue.svg)](https://atomgit.com/choked-up-now/JTWin/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows-lightgrey.svg)]()
[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Architecture](https://img.shields.io/badge/architecture-HTTP%20%2F%20P2P-brightgreen.svg)]()

极致轻量、多线程并发的现代化跨平台下载引擎。基于 Rust 编写，兼顾极客的命令行效率与普通用户的图形界面体验。

## 核心特性

- **极致轻量**：原生编译，拒绝臃肿。提供带图形界面的独立可执行文件，体积仅数 MB。
- **双端架构**：通过 Cargo Features 实现依赖隔离，可按需编译独立的 CLI 与 GUI。
- **纯 HTTP 极速下载**：支持 Range 请求、多线程分片下载、字节级断点续传（`.jtdl` 文件）。
- **智能镜像加速**：内置常用开源镜像规则，下载前自动并发测速，无缝切换至最优节点。
- **原生 P2P 引擎**：手写实现 BEP-3、BEP-15、BEP-41 协议。支持磁力链接解析、UDP Tracker 节点发现、MSE/RC4 加密握手。
- **双轨更新通道**：支持正式版（Latest）与预览版（Alpha/Beta）的自动更新检测与无感静默升级。
- **按需定制安装**：安装向导提供组件选择，用户可自由决定是否安装 P2P 种子下载支持，避免功能冗余。

## 安装指南

请前往 [Releases](https://atomgit.com/choked-up-now/JTWin/releases) 页面下载最新的安装包（`JiTuan_vX.Y.Z_Setup.exe`）。

安装向导中提供以下可选组件：
- **图形界面版 (GUI)**：提供直观的下载控制台。
- **命令行版 (CLI)**：自动加入系统环境变量 `PATH`，适合脚本与极客用户。
- **P2P 种子/磁力链下载支持**：如果只需纯 HTTP 下载，可取消勾选此项，获得极致精简的安装包。

## 使用指南

### GUI 桌面模式
双击运行 `jituan-gui.exe`。支持粘贴链接自动提取文件名、自定义保存路径、一键管理镜像规则，以及无感静默更新。

### CLI 命令行模式
```bash
# 基础 HTTP 下载
jituan-cli https://example.com/file.zip

# 指定保存路径与并发数
jituan-cli https://example.com/file.zip -o D:\Downloads -c 32

# 下载磁力链接（需包含 P2P 组件）
jituan-cli "magnet:?xt=urn:btih:..."

# 检查并接收预览版更新
jituan-cli --update --alpha
```

## 从源码构建

本项目使用标准的 Rust 工具链，确保已安装 Rust 环境。

```bash
# 编译完整版（包含 GUI、CLI 与 P2P 功能）
cargo build --release --features full

# 编译纯 HTTP 精简版
cargo build --release --features cli,gui
```
编译产物将生成在 `target/release/` 目录下。

### Cargo Features 说明
- `cli`：启用命令行界面（依赖 `clap`）。
- `gui`：启用图形界面（依赖 `eframe`）。
- `p2p`：启用 P2P 底层协议支持（依赖 `bendy`, `rc4`, `num-bigint` 等）。
- `full`：包含以上所有功能。

## 技术架构

- **网络层**：`reqwest` + `tokio`（支持 HTTP/2 与原生异步 IO）。
- **调度层**：自研 `Semaphore` 令牌桶，动态计算并发配额。
- **P2P 层**：纯手写 BitTorrent 协议栈，支持 Diffie-Hellman 密钥交换与 RC4 流加密。
- **界面层**：`eframe` (egui) 原生渲染。
- **自动化**：配套 PowerShell CI/CD 流水线，一键完成编译、打包、Git 推送与 Release 发布。

## 开源协议

本项目采用 [Apache License 2.0](LICENSE) 开源协议。
```
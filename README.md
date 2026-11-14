# 汲湍 (JiTuan)

[![Version](https://img.shields.io/badge/version-0.11.0-blue.svg)](https://atomgit.com/choked-up-now/JTWin/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows-lightgrey.svg)]()
[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Architecture](https://img.shields.io/badge/architecture-HTTP%20%2F%20P2P-brightgreen.svg)]()

极致轻量、多线程并发的现代化跨平台下载引擎。基于 Rust 编写，兼顾极客的命令行效率与普通用户的图形界面体验。

## 核心特性

- **极致轻量**：原生编译，拒绝臃肿。提供带图形界面的独立可执行文件，体积仅数 MB。
- **纯 HTTP 极速下载**：支持 Range 请求、多线程分片下载、字节级断点续传（`.jtdl` 文件）。
- **智能线程动态调节 (AIMD)**：基于微块任务队列，在带宽充足时自动增加线程，在触发风控/限速时立刻线程减半，完美平衡速度与封禁风险。
- **纯手写原生 P2P 引擎**：底层全面接入成熟网络栈，原生支持 DHT 去中心化节点发现、UDP 打洞。实测公网磁力链下载速度可突破 35 MB/s。
- **自动化动作引擎 (Actions)**：下载完成后，可根据文件后缀名自动调用外部程序（如 7-Zip 静默解压、调用指定播放器），真正实现“下载即用”。
- **原生系统语言跟随**：自动检测系统语言，支持外挂 JSON 语言包（AppData 目录下），瞬时切换无需重启。
- **按需定制安装**：安装向导提供组件选择，用户可自由决定是否安装 P2P 种子下载支持，避免功能冗余。

## 安装指南

请前往 [Releases](https://atomgit.com/choked-up-now/JTWin/releases) 页面下载最新的安装包（`JiTuan_vX.Y.Z_Setup.exe`）。

安装向导中提供以下可选组件：
- **图形界面版 (GUI)**：提供直观的下载控制台。
- **命令行版 (CLI)**：自动加入系统环境变量 `PATH`，适合脚本与极客用户。
- **P2P 种子/磁力链下载支持**：如果只需纯 HTTP 下载，可取消勾选此项，获得极致精简的安装包。

## 使用指南

### GUI 桌面模式
双击运行 `jituan-gui.exe`。支持粘贴链接自动提取文件名、自定义保存路径、镜像测速，以及下载完成后的“⚡ 智能执行”一键解压/打开。

### CLI 命令行模式
```bash
# 基础 HTTP 下载
jituan-cli https://example.com/file.zip

# 下载磁力链接（需包含 P2P 组件）
jituan-cli "magnet:?xt=urn:btih:..."

# 下载完成后自动执行外部动作（如调用 7-Zip 解压）
jituan-cli https://example.com/file.zip --action
```

### 自动化规则 (actions.json)
首次运行会在 `C:\Users\你的用户名\AppData\Roaming\JiTuan\` 释放 `actions.json` 配置文件。你可以自由定义后缀与程序的映射：
```json
[
  {
    "ext": "zip",
    "action": "execute",
    "app_path": "C:\\Program Files\\7-Zip\\7z.exe",
    "args": ["x", "{file}", "-o{dir}", "-y"]
  }
]
```
*(占位符 `{file}` 代表下载的文件路径，`{dir}` 代表保存目录)*

## 从源码构建

本项目使用标准的 Rust 工具链，确保已安装 Rust 环境。

```bash
# 编译完整版（包含 GUI、CLI 与 P2P 功能）
cargo build --release --features full

# 编译纯 HTTP 精简版
cargo build --release --features cli,gui
```
编译产物将生成在 `target/release/` 目录下。

## 开源协议

本项目采用 [Apache License 2.0](LICENSE) 开源协议。
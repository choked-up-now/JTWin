# 汲湍 (JiTuan)

极致轻量、动态并发的跨平台下载引擎。基于 Rust 编写，兼顾极客的命令行效率与普通用户的图形界面体验。

## 核心特性

- 极致轻量：抛弃传统 Electron 架构，原生编译，GUI 体积仅 5MB 左右，CLI 体积不足 3MB。
- 动态并发调度：基于全局信号量（Semaphore）控制，根据当前活跃任务数自动计算单任务并发分片，避免网络拥塞。
- 断点续传：下载进度实时持久化，独创 `.jtdl` 专属断点文件，支持随时暂停与无缝恢复。
- 双端合一：通过 Cargo Features 实现依赖隔离，可按需编译出互不干扰的 CLI 与 GUI 可执行文件。
- 原生级网络体验：Windows 端默认使用系统原生 TLS（Schannel）与防封锁 User-Agent 策略。

## 快速开始

### 方式一：使用安装包（推荐）
前往 [Releases](https://atomgit.com/choked-up-now/JiTuan/releases) 页面下载最新的安装包（`.exe`）。
安装过程中，你可以选择仅安装图形界面、仅安装命令行工具，或者两者同时安装。若勾选命令行版本，安装程序会自动将其加入系统 `PATH` 环境变量。

### 方式二：从源码编译
请确保已安装 Rust 开发环境（支持 GNU 或 MSVC 工具链）。

1. 克隆仓库：
   ```bash
   git clone https://atomgit.com/choked-up-now/JiTuan.git
   cd JiTuan
   ```

2. 编译 GUI 桌面版（生成 `target/release/jituan-gui.exe`）：
   ```bash
   cargo build --release --features gui --bin jituan-gui
   ```

3. 编译 CLI 命令行版（生成 `target/release/jituan-cli.exe`）：
   ```bash
   cargo build --release --features cli --bin jituan-cli
   ```

## 使用指南

### CLI 命令行模式
```bash
# 基础用法（默认 16 并发分片）
jituan-cli.exe <下载链接>

# 指定保存文件名与并发连接数
jituan-cli.exe https://speed.cloudflare.com/__down?bytes=100000000 -o test.bin -c 16
```

### GUI 桌面模式
双击运行 `jituan-gui.exe`，将下载链接粘贴至输入框，点击“开始下载”即可。界面会实时反馈下载进度。

## 技术架构

- 网络层：`reqwest` + `tokio`（支持 HTTP/2 与原生异步 IO）
- 调度层：自定义 `Semaphore` 令牌桶，动态计算 `perTask` 配额
- 界面层：`eframe` (egui) + 纯 Rust 原生渲染，自动调用系统字体
- 持久化：`serde_json` 配合自定义 `.jtdl` 格式记录字节级断点

## 开源协议

本项目采用 [Apache License 2.0](LICENSE) 开源协议。使用本项目相关依赖及系统资源时，请遵守 [NOTICE](NOTICE) 文件中的相关规定。
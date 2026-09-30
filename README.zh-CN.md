<div align="center">

<img src="assets/icon-256.png" width="88" alt="MageKit 图标" />

# MageKit

**视频下载、网页媒体嗅探与直播录制，一站式桌面工具。**

基于 Rust、GPUI Fast 和 GPUI Kit 构建的原生媒体工具箱。

[下载安装](https://github.com/emojiiii/magekit-app/releases/latest) · [功能介绍](#功能介绍) · [源码构建](#源码构建) · [开发文档](#开发文档) · [English](readme.md)

</div>

![MageKit 中文下载页面，浅色主题](docs/screenshots/home-light-zh.png)

## 项目简介

MageKit 将视频下载、频道浏览、网页媒体嗅探和直播录制整合到一个桌面工作区。应用结合原生平台接口与 yt-dlp、FFmpeg、Streamlink，并提供统一的工具和任务管理界面。

界面支持 **English、简体中文和跟随系统**，语言选择可持久保存，并提供深色、浅色主题。本文介绍当前 `main` 源码的功能，已发布安装包可能落后于最新源码。

## 功能介绍

| 工作区 | 主要功能 |
| --- | --- |
| **视频下载** | 解析支持的媒体链接，查看可用格式，选择音视频选项并加入下载队列 |
| **频道下载** | 浏览支持的频道页面，将选中的视频加入队列 |
| **资源嗅探** | 通过静态检查和浏览器会话发现网页媒体资源 |
| **直播录制** | 管理直播间、监控支持的直播流、手动或自动录制，支持搜索及网格/列表视图 |
| **任务管理** | 查看进度和历史、设置并发数，按任务能力暂停、恢复、重试或取消 |
| **工具管理** | 检测、安装和更新 yt-dlp、FFmpeg、Deno 及受管理的 Streamlink 环境 |
| **偏好设置** | 配置输出目录、格式、字幕、代理、平台 Cookie、界面语言和主题 |

### 平台支持说明

- 视频下载优先使用已实现的原生接口，其他站点由 yt-dlp 处理，包括 YouTube、Bilibili、X 等。能否下载取决于具体链接、解析器版本、账号权限和站点状态。
- 直播录制使用抖音原生录制器；其他已支持的平台通过 Streamlink 处理，包括 Bilibili、斗鱼、虎牙、SOOP 等集成。出现平台或房间标签不代表 Streamlink 一定支持该链接；当前不支持快手录制。
- Cookie 仅用于访问账号本身有权观看的内容，不提供绕过 DRM、付费权限或其他访问限制的能力。

## 应用截图

截图来自 Linux 上实际运行的应用窗口，对应已合入 `main` 的 [`84dff99`](https://github.com/emojiiii/magekit-app/commit/84dff99fd8049e06b7fde524a00aaaaac0ac1d64) 源码树。使用隔离的演示配置，不包含用户账号或真实录制会话。不同操作系统和旧版安装包的外观可能不同。

### 英文界面 · 深色主题

![MageKit 英文下载页面，深色主题](docs/screenshots/home-dark-en.png)

## 下载安装

从 [GitHub Releases](https://github.com/emojiiii/magekit-app/releases) 下载对应版本实际提供的文件：

| 平台 | 安装包 |
| --- | --- |
| **Windows x64** | 独立 `.exe` 或便携版 `.zip` |
| **macOS · Intel / Apple Silicon** | 通用 `.dmg` 或 `.app.zip` |
| **Linux x64** | `.AppImage` 或 `.tar.gz` |

具体系统要求以对应版本的发布说明为准。Linux 需要可用的图形驱动及 GPUI 所需系统库；下载的 AppImage 需先赋予可执行权限。解压压缩包后，请保留随附的资源文件。

### 首次使用

1. 打开「设置」，选择输出目录、界面语言和主题。
2. 在「工具管理」检查 yt-dlp 和 FFmpeg，按需安装或更新。
3. 在「视频下载」粘贴媒体链接，解析后选择可用格式并开始下载。
4. 前往「下载任务」查看进度和管理任务。

YouTube 的 JavaScript challenge 可能需要 Deno。MageKit 可从官方发布源安装，并在工具页提供更新/修复入口；官方 yt-dlp 独立发行版已包含 EJS 脚本。

录制非抖音平台时，需在工具页安装 **Streamlink** 环境。首次安装需要联网下载受管理的 Python 和相关依赖。Release 构建内嵌的是 uv 引导程序，不是完整离线录制环境，详见[录制运行环境说明](docs/streamlink.md)。

## 源码构建

### 环境要求

- 支持 **Rust edition 2024** 的当前稳定版工具链，以及 Git
- **Python 3**，用于录制模块的构建辅助脚本和翻译检查
- **Windows：** Visual Studio C++ 构建工具与 Windows SDK
- **macOS：** Xcode Command Line Tools
- **Linux：** C/C++ 工具链与 GPUI 所需原生库

Ubuntu/Debian 可参考当前 CI 的依赖列表：

```bash
sudo apt-get update
sudo apt-get install -y build-essential clang cmake ninja-build pkg-config libssl-dev \
  libfontconfig1-dev libfreetype6-dev libxkbcommon-dev libxkbcommon-x11-dev \
  libwayland-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev \
  libxcb-xfixes0-dev libx11-dev libx11-xcb-dev libasound2-dev \
  libvulkan-dev libegl1-mesa-dev
```

### 运行与构建

```bash
git clone https://github.com/emojiiii/magekit-app.git
cd magekit-app
cargo run --locked --bin magekit

# 构建优化后的应用
cargo build --release --locked --bin magekit
```

产物位于 `target/release/magekit`，Windows 下为 `magekit.exe`。开发时请从仓库根目录运行，以便加载主题资源。

Release 构建会下载并校验与目标平台匹配的 uv 可执行文件。开发构建不内嵌 uv；测试 Streamlink 时需自行安装 uv 或设置 `MAGEKIT_UV_PATH`。离线构建覆盖项和交叉编译说明见 [docs/streamlink.md](docs/streamlink.md)。

### 常用检查

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --locked --bin magekit
python3 scripts/check_i18n.py
```

以上覆盖格式、编译、应用回归与翻译一致性，不等同于完整集成测试。部分 workspace 测试依赖真实网站或外部媒体工具。[CI 工作流](.github/workflows/ci.yml)、[Streamlink 工作流](.github/workflows/streamlink.yml)和[验证记录](docs/security-ux-review.md)分别说明覆盖范围及已知失败；最新结果见 [Actions](https://github.com/emojiiii/magekit-app/actions)。

## 项目结构

```text
src/                    原生应用、页面、状态和国际化
crates/
  shared/               共享配置、类型与路径
  tool_manager/         工具安装、任务调度与持久化
  download/             下载引擎与进度处理
  extractor/            媒体信息与平台解析
  platform_api/         平台 API 集成
  capture/              网页媒体发现
  live_recorder/        录制与 Streamlink 运行环境
  xbogus/               平台请求签名辅助组件
locales/                中英文翻译源文件
themes/                 主题定义
scripts/                翻译、构建与发布脚本
docs/                   架构与使用说明
```

## 开发文档

- [应用架构](docs/gui_app.md)
- [GPUI Fast / GPUI Kit 迁移说明](docs/gpui-migration.md)
- [语言选择与翻译维护](docs/localization.md)
- [直播录制与 Streamlink 运行环境](docs/streamlink.md)
- [安全、交互与验证记录](docs/security-ux-review.md)
- [开发指南](agents.md)
- [发布工作流](.github/workflows/release.yml)

推送到 `main` 后，发布工作流会计算版本；纯文档变更和带 `[skip release]` 的提交会跳过发版。各平台构建和产物检查通过后，才会创建标签与 Release。

## 常见问题与使用须知

- **下载失败：** 检查链接、可用格式、工具版本、代理和账号权限。站点变更可能需要更新解析器。
- **录制失败：** 确认 FFmpeg 和录制环境已安装、直播流可访问、输出目录可写；反馈时提供平台和错误发生阶段。
- **应用无法启动：** 检查系统库和图形驱动支持，反馈时附上操作系统及应用版本。
- **提交问题：** 日志和截图中请移除 Cookie、Token、签名链接和个人路径。配置文件可能包含账号敏感信息，不要提交到仓库或直接分享。

请仅下载或录制你拥有或获准使用的内容，并遵守来源平台的使用条款。

## 贡献与许可证

欢迎提交 [Issue](https://github.com/emojiiii/magekit-app/issues) 和范围清晰的 Pull Request。请提供复现步骤和相关检查结果；界面变更请附截图，并注明语言与主题。

`Cargo.toml` 声明 **MIT OR Apache-2.0**。独立的项目许可证文件尚未补齐，重新分发前请查看仓库当前许可材料。内嵌工具和依赖保留各自的许可证。

感谢 [GPUI Fast](https://github.com/longbridge/gpui-fast)、[GPUI Kit](https://github.com/longbridge/gpui-kit)、[yt-dlp](https://github.com/yt-dlp/yt-dlp)、[FFmpeg](https://ffmpeg.org/)、[Streamlink](https://github.com/streamlink/streamlink)、[uv](https://github.com/astral-sh/uv) 和 [Tokio](https://tokio.rs/)。

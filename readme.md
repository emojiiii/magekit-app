# MageKit 视频下载器

<div align="center">

一个基于 Rust 和 GPUI 构建的现代化视频下载桌面应用程序

[![Rust](https://img.shields.io/badge/Rust-2024-orange.svg)](https://www.rust-lang.org/)
[![GPUI](https://img.shields.io/badge/GPUI-Latest-blue.svg)](https://www.gpui.rs/)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

[功能特性](#功能特性) • [快速开始](#快速开始) • [构建说明](#构建说明) • [使用指南](#使用指南) • [开发文档](#开发文档)

</div>

---

## 📸 应用截图

<img width="1500" height="999" alt="MageKit 主界面" src="https://github.com/user-attachments/assets/49d49c5e-4b1e-4e64-b51a-7e5d666e91c7" />

<img width="1500" height="999" alt="MageKit 设置页面" src="https://github.com/user-attachments/assets/d10e8016-f106-4cd7-bf9e-8bbab4e9224e" />

---

## 📖 项目简介

MageKit 是一个功能强大、界面美观的视频下载工具，支持从各大视频平台下载内容。基于 Rust 语言开发，使用 GPUI 框架提供流畅的 GPU 加速用户界面，采用 yt-dlp 作为核心下载引擎。

### ✨ 功能特性

- 🎬 **多平台支持** - 支持 YouTube、Bilibili、Twitter 等主流视频平台
- 📥 **智能下载** - 自动识别视频格式，支持自定义质量选择
- 🎨 **主题系统** - 内置多款精美主题，支持亮色/暗色模式切换
- ⚡ **并发下载** - 支持多任务并发，可自定义并发数量
- 📋 **任务管理** - 完整的下载历史记录和任务状态跟踪
- 🔧 **工具管理** - 自动检测、安装和更新 yt-dlp、ffmpeg 等工具
- 🍪 **Cookie 支持** - 支持配置平台 Cookie 以访问会员内容
- 🌐 **代理设置** - 支持系统代理和自定义 HTTP/SOCKS5 代理
- 💾 **元数据嵌入** - 自动嵌入视频元数据和缩略图
- 🔄 **状态持久化** - 任务状态自动保存，应用重启后恢复

### 🖥️ 系统要求

#### 支持的操作系统
- Windows 10/11 (x64)
- macOS 10.15+ (Intel/Apple Silicon)
- Linux (主流发行版)

#### 硬件要求
- CPU: 多核处理器推荐
- 内存: 至少 4GB RAM
- 显卡: 支持 OpenGL 3.3+ 或 Metal/DirectX 11+
- 磁盘: 至少 500MB 可用空间

---

## 🚀 快速开始

### 1. 安装依赖

#### Rust 工具链
首先安装 Rust（推荐使用 rustup）：

```bash
# 安装 rustup（Windows）
# 访问 https://rustup.rs/ 下载安装器

# 安装 rustup（macOS/Linux）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 验证安装
rustc --version
cargo --version
```

#### 系统依赖

**Windows:**
- 安装 [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/) 或 Visual Studio（需要 C++ 工作负载）

**macOS:**
```bash
# 安装 Xcode Command Line Tools
xcode-select --install
```

**Linux (Ubuntu/Debian):**
```bash
sudo apt update
sudo apt install build-essential pkg-config libssl-dev \
  libfontconfig1-dev libfreetype6-dev libxcb-render0-dev \
  libxcb-shape0-dev libxcb-xfixes0-dev
```

**Linux (Fedora):**
```bash
sudo dnf install gcc gcc-c++ make pkgconfig openssl-devel \
  fontconfig-devel freetype-devel libxcb-devel
```

### 2. 克隆项目

```bash
git clone https://github.com/yourusername/magekit-app.git
cd magekit-app
```

### 3. 运行项目

#### 开发模式运行
```bash
# 方式一：在根目录运行
cargo run --bin magekit

# 方式二：进入 gui_app 目录运行
cd gui_app
cargo run
```

#### 发布模式运行（性能优化）
```bash
cargo run --release --bin magekit
```

---

## 🔨 构建说明

### 开发构建

```bash
# 构建所有 workspace 成员
cargo build

# 仅构建 GUI 应用
cargo build --bin magekit

# 构建特定包
cargo build -p magekit-tool-manager
cargo build -p magekit-shared
```

### 发布构建

```bash
# 构建优化版本
cargo build --release --bin magekit

# 构建产物位置
# Windows: target/release/magekit.exe
# macOS/Linux: target/release/magekit
```

### 构建配置优化

项目已在 `Cargo.toml` 中配置了针对 GPUI 的优化设置：

```toml
[profile.dev]
codegen-units = 16
debug = "limited"
split-debuginfo = "unpacked"

[profile.dev.package]
resvg = { opt-level = 3 }
rustybuzz = { opt-level = 3 }
taffy = { opt-level = 3 }
ttf-parser = { opt-level = 3 }
```

### 检查和测试

```bash
# 代码检查
cargo check

# 运行测试
cargo test

# 代码格式化
cargo fmt

# 代码 lint
cargo clippy
```

---

## 📚 使用指南

### 首次启动

1. **启动应用**：运行编译好的可执行文件
2. **工具检测**：应用会自动检测 yt-dlp 和 ffmpeg，如未安装会提示安装
3. **配置设置**：
   - 点击左侧侧边栏的 "设置" 图标
   - 配置默认下载目录
   - 选择喜欢的主题
   - 根据需要配置代理和 Cookie

### 下载视频

1. **添加下载**：
   - 点击 "频道" 页面
   - 在输入框中粘贴视频 URL
   - 点击 "添加下载" 按钮

2. **配置下载选项**：
   - 选择视频质量（最佳/高/中/低）
   - 选择是否下载字幕
   - 选择输出格式

3. **开始下载**：
   - 任务自动加入下载队列
   - 实时查看下载进度
   - 支持暂停/恢复/取消

### 配置 Cookie（访问会员内容）

1. 打开浏览器，登录目标平台
2. 按 F12 打开开发者工具
3. 进入 Application → Cookies → 复制 Cookie 内容
4. 在 MageKit 设置页面的 "平台 Cookie" 区域：
   - 输入平台名称（如：bilibili、youtube）
   - 粘贴 Cookie 内容
   - 点击 "添加"

### 主题切换

- 进入设置页面
- 在 "外观设置" 区域选择主题
- 主题会立即生效并自动保存

---

## 🏗️ 项目结构

```
magekit-app/
├── gui_app/                 # GUI 主应用
│   ├── src/
│   │   ├── app/            # 应用核心逻辑
│   │   ├── ui/             # UI 组件
│   │   │   ├── pages/      # 页面组件
│   │   │   └── widgets/    # 通用组件
│   │   ├── theme/          # 主题配置
│   │   └── main.rs         # 入口文件
│   └── Cargo.toml
├── tool_manager/           # 工具管理模块
│   ├── src/
│   │   ├── task_manager.rs # 任务调度器
│   │   ├── downloader.rs   # 下载器实现
│   │   └── tool_installer.rs # 工具安装器
│   └── Cargo.toml
├── shared/                 # 共享类型和工具
│   ├── src/
│   │   ├── types.rs        # 共享数据类型
│   │   ├── config.rs       # 配置管理
│   │   └── lib.rs
│   └── Cargo.toml
├── themes/                 # 主题文件目录
│   ├── dark.toml
│   ├── light.toml
│   └── ...
├── Cargo.toml             # Workspace 配置
├── Cargo.lock
├── README.md
└── CHANGELOG.md
```

### 模块说明

#### `gui_app` - GUI 应用
- 基于 GPUI 框架的桌面应用界面
- 实现路由、状态管理、UI 渲染
- 包含所有页面和组件

#### `tool_manager` - 工具管理
- 下载任务的调度和执行
- yt-dlp、ffmpeg 的检测、安装和更新
- 进程管理和输出解析

#### `shared` - 共享库
- 跨模块的公共类型定义
- 配置文件读写
- 工具函数和常量

---

## 🛠️ 开发文档

### 开发环境设置

推荐使用以下 IDE 和插件：

- **Visual Studio Code** + rust-analyzer 插件
- **RustRover / IntelliJ IDEA** + Rust 插件
- **Vim/Neovim** + rust.vim + coc-rust-analyzer

### 代码风格

项目遵循 Rust 官方代码规范：

```bash
# 格式化代码
cargo fmt

# 检查代码质量
cargo clippy -- -D warnings
```

### 添加新主题

1. 在 `themes/` 目录创建新的 `.toml` 文件
2. 参考现有主题配置颜色值
3. 重启应用即可在设置中选择新主题

示例主题配置：
```toml
name = "我的主题"
mode = "dark"

[colors]
background = "#1e1e2e"
foreground = "#cdd6f4"
primary = "#89b4fa"
# ... 其他颜色配置
```

### 调试技巧

启用详细日志输出：
```bash
RUST_LOG=debug cargo run
```

### 贡献指南

欢迎提交 Issue 和 Pull Request！

1. Fork 本项目
2. 创建特性分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 开启 Pull Request

---

## 📝 常见问题

### Q: 构建失败，提示找不到系统库？
A: 请确保已安装所有系统依赖，参考 [安装依赖](#1-安装依赖) 部分。

### Q: 应用启动后崩溃？
A: 检查显卡驱动是否支持 OpenGL 3.3+，更新到最新版本驱动程序。

### Q: 下载失败提示工具不可用？
A: 在设置页面检查 yt-dlp 和 ffmpeg 状态，点击安装或更新按钮。

### Q: 如何下载需要登录的视频？
A: 在设置页面的 "平台 Cookie" 区域配置对应平台的 Cookie。

### Q: 下载速度慢？
A: 可以在设置中配置代理服务器，或调整并发下载数量。

---

## 📄 开源协议

本项目采用双重许可：

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

您可以选择其中任意一种许可协议。

---

## 🙏 致谢

- [GPUI](https://www.gpui.rs/) - 现代化的 Rust GUI 框架
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) - 强大的视频下载工具
- [FFmpeg](https://ffmpeg.org/) - 多媒体处理框架
- [Tokio](https://tokio.rs/) - 异步运行时

---

## 📮 联系方式

- 项目主页: [GitHub](https://github.com/yourusername/magekit-app)
- 问题反馈: [Issues](https://github.com/yourusername/magekit-app/issues)
- 更新日志: [CHANGELOG.md](CHANGELOG.md)

---

<div align="center">

**[⬆ 返回顶部](#magekit-视频下载器)**

Made with ❤️ using Rust and GPUI

</div>

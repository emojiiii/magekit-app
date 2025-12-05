好的 — 下面是一个 **Markdown 格式** 的 “产品 / 项目规范说明 (SPEC / README 模板)” 草稿。你可以拿这个直接作为项目根目录下的 `README.md` 或 `SPEC.md`，也可以根据需要改／补充。

---

# Rust-Downloader GUI 应用 / Tool-Manager 项目

> **项目名称 (示例)**: `rust_video_downloader`

## 🎯 项目简介

`rust_video_downloader` 是一个跨平台 (Windows / macOS / Linux) 的桌面应用，使用 **GPUI + GPUI Component** 构建 GUI 界面，后端通过一个自定义的 “工具管理 + 下载逻辑库 (tool-manager)” 来自动管理下载 / 更新 **yt-dlp / ffmpeg** 等外部二进制工具，并调用它们完成视频／音频下载与转码。最终目标是让终端用户无需 Python / 手动安装，只需一个可执行文件即可进行下载。

主要特点：

- 原生 Rust + GPU 加速 GUI (性能 + 跨平台)
- 自动管理 yt-dlp / ffmpeg 等工具 (下载 / 更新 /缓存)
- 提供简单易用 GUI 界面 (URL 输入、格式选择、下载队列、状态/进度展示)
- 支持多任务 / 并发 /下载队列 /暂停 /取消 /状态追踪
- 支持配置 / 设置 (输出目录 /默认下载选项 /自动更新 /主题等)
- 最终发布为单一二进制 (或带打包 installer/app)，对用户友好

---

## 📦 项目结构 & 模块划分

```
rust_video_downloader/
│
├── tool_manager/        # 后端库 (crate)
│   ├── src/
│   │    ├── downloader.rs   # 调用 yt-dlp + ffmpeg，执行下载/转码
│   │    ├── updater.rs      # 检查/下载/更新 yt-dlp / ffmpeg 等工具
│   │    ├── storage.rs      # 二进制缓存管理 (路径 /版本 /cache)
│   │    ├── config.rs       # 配置管理 (默认选项 /用户设置)
│   │    └── error.rs        # 错误类型定义 & 统一错误处理
│   └── Cargo.toml
│
├── gui_app/              # GUI 应用 (binary)
│   ├── src/
│   │    ├── main.rs         # 应用入口，初始化 GPUI + window
│   │    ├── ui/             # UI 相关模块 (views, components, 状态管理)
│   │    └── logic/          # 前端逻辑 (调用 tool_manager, 任务队列管理)
│   └── Cargo.toml
│
├── README.md             # 本说明文档
├── LICENSE (MIT / Apache)
└── .gitignore
```

> **说明**: 你也可以把 `tool_manager` 放在同一个 crate 里 (workspace)，不过分离后端库 + GUI 的结构更清晰、可复用 (比如将来添加 CLI、daemon、插件等)。

---

## 🔧 技术选型与依赖

- GUI 框架： `gpui = "*"` + `gpui-component = "*"`([longbridge.github.io][1])
- UI 组件：GPUI Component 提供丰富组件 (按钮、输入框、表格 / 列表 /虚拟列表 /表格 /布局 /主题 /图标 /对话框 /弹窗 /Markdown 渲染 /表格等)([GitHub][2])
- 后端工具下载 /更新机制：自行实现，用于自动下载/缓存/版本管理 yt-dlp / ffmpeg 等
- Rust 版本 /编译环境：要求 stable Rust + Cargo。GPUI 自身还在活跃开发中 (pre-1.0)，需要留意 breaking changes。([Docs.rs][3])

---

## 🖥️ UI 界面 / 功能设计

### 主窗口 (Main Window)

- 顶部菜单 (File / Help / Settings / About)
- 主内容区分为两个 Panel：

  - **下载面板 (Download Panel)**

    - URL 输入框 (支持粘贴 /多行 /批量)
    - “Fetch Info” 按钮 (获取视频／音频信息)
    - 视频信息展示区 (Title, Duration, Available formats / qualities, size estimate 等)
    - 选项控件 (下拉菜单 /复选框 /输入框)：选择格式 (mp4 / mp3 / 其他), 清晰度 / 质量, 输出目录 (带浏览对话框)
    - “Download” 按钮

  - **任务 / 下载队列 & 状态展示面板**

    - 任务列表 (表格 /列表)，每行显示任务：URL /文件名 /状态 (Queued / In-Progress / Completed / Error) /进度条 /大小 /速度 /剩余时间
    - 每个任务提供按钮 (Pause / Cancel / Open Folder / Retry)
    - 支持多任务 /并发 /队列 /顺序下载 /暂停 /取消

### 设置 / Preferences 窗口

- 默认输出目录
- 是否自动更新 yt-dlp / ffmpeg (启动时 / 手动确认)
- 并发下载数 /队列策略 (串行 /并发 /最大任务数)
- 主题 /界面语言 (light / dark / system / custom)

### 日志 / 控制台 /输出窗口 (可选)

- 展示后台执行日志 /错误 /调试信息 /ffmpeg 输出 /yt-dlp 输出
- 对高级用户 /排错者有用

### 关于 (About) 窗口

- 应用版本号 (你的 GUI 程序版本)
- 后端工具版本 (yt-dlp / ffmpeg 版本)
- 许可证 /作者 /链接 /反馈方式

---

## 🔄 后端逻辑 & 功能 (Tool-Manager)

### 核心功能

- 检查本地是否已有工具 (yt-dlp / ffmpeg)，若无或版本过旧 → 自动下载适合当前平台 (Windows / macOS / Linux) 的 binary
- 管理二进制缓存 (存放位置、版本记录)
- API 方法供 GUI 调用：

  - `ensure_tools()` — 确保工具就绪 /最新 /或符合版本约定
  - `get_video_info(url: &str) -> VideoInfo` — 获取媒体信息 (格式 / 清晰度 / 大小 /元数据)
  - `download(url: &str, options: DownloadOptions) -> DownloadHandle` — 启动下载任务 (异步 / 多任务)
  - `pause / resume / cancel / retry / query_status` 等任务控制 API
  - `update_tools(channel: UpdateChannel)` — 手动 /自动更新工具 (stable / latest / pinned)

### 配置与持久化

- 配置文件 (例如 `config.toml` / `config.json`)，保存用户设置 (输出路径 /自动更新 /并发数 /默认选项 /theme /语言)
- 存储工具缓存 (binary) 与版本信息 (metadata) 在用户目录 (例如：Windows: `%APPDATA%/rust_video_downloader/tools`; macOS/Linux: `~/.local/share/...` 或 `~/Library/Application Support/...`)

### 错误处理 / 日志 /状态反馈

- 明确错误类型 (下载失败 /网络 /权限 /binary 不兼容 /ffmpeg 错误 /yt-dlp 错误)
- 将错误 /日志 /状态通过 callback / channel / message 等机制传递给 GUI，供界面展示 /日志窗口使用

---

## 🛠️ 开发 /构建 /发布流程

1. 使用 `cargo workspace`，同时管理 `tool_manager` 和 `gui_app` 两个 crate。
2. GUI 部分使用 `gpui + gpui-component`。可考虑用 `create-gpui-app` 脚手架快速启动项目骨架。([crates.org.cn][4])
3. 后端实现下载 /更新逻辑 (建议先实现基本的 `ensure_tools()` + `download(url)`，做最简可用版本)
4. GUI 实现最简单界面 (URL 输入 + Download) + 状态展示，做 end-to-end 测试 (确保从启动 → 下载 → 文件输出能流畅工作)
5. 添加任务管理 (队列 /并发 /暂停 /取消) + 设置 /配置持久化 +自动/手动工具更新机制
6. 测试不同平台 (Windows / macOS / Linux)，确认工具下载 / binary 可用 /兼容性
7. 打包 / 发布：对于 Windows 可生成 installer (NSIS / Squirrel /类似)，macOS 可打包为 `.app`，Linux 可打 tar.gz / AppImage /Flatpak，亦可仅发布二进制 + “首次启动自动下载工具 +运行”。

---

## 🚧 风险 / 注意事项

- **依赖 GPUI / GPUI-Component 生态成熟度**：GPUI 是较新的 Rust GUI 框架，目前还在活跃开发中，可能存在 breaking changes /文档不完善 /兼容性问题。社区里有人提到使用体验不错，但文档不够完善，需要读源码或 demo。([reddit.com][5])
- **工具 (yt-dlp / ffmpeg) 更新频繁**：需要设计合理的版本检测与更新机制，避免自动更新导致兼容性问题或用户中断下载。
- **跨平台差异**：不同 OS 对 binary 的调用方式、权限、存放路径、文件系统等可能不同，需要小心处理。
- **打包体积**：尽管是 Rust +GPU GUI，但若 bundling ffmpeg / yt-dlp，最终体积可能比较大 (取决于 ffmpeg 的裁剪与优化)
- **许可证 /合规性**：如果你发布给他人使用，需要注意 yt-dlp / ffmpeg 的使用 /分发许可 (尤其 ffmpeg 的编解码器许可 /专利问题)

---

## 📈 项目发展 / Milestone (Roadmap)

| 阶段                                 | 内容                                                                                                                        |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------------------- |
| **阶段 0 — 原型 (Proof of Concept)** | 实现 `tool_manager.ensure_tools()` + `download(url)` + 简单 GUI (URL 输入 + Download) + 输出文件 → 成功下载 & 保存即可      |
| **阶段 1 — 基础功能完善**            | 支持多任务 /并发 /队列 /状态 /任务列表 /暂停／取消／错误处理 + GUI 任务列表展示                                             |
| **阶段 2 — 配置与设置**              | 实现用户设置界面 (默认目录 /并发数 /自动更新 /主题 /语言) + 配置持久化 + 自动 / 手动工具更新                                |
| **阶段 3 — UI / UX 完善**            | 完善 GUI (格式 /清晰度选择 /输出目录选择 /主题 /译名 /多语言) + 下载日志 /输出 /进度 /通知 /错误提示 + About /帮助窗口      |
| **阶段 4 — 跨平台构建与打包**        | Windows / macOS / Linux 的 Release 构建 + 打包 / installer /App / AppImage 等发布方式                                       |
| **阶段 5 — 扩展 / 功能增强**         | 批量下载 / 导入 URL 列表 /订阅 /历史记录 /下载管理 /插件系统 /未来支持其他网站或外部 downloader backend /用户反馈 /社区支持 |

---

## 📄 许可 & 贡献指南

- 本项目采用 **MIT 或 Apache-2.0** (或两者双许可)
- 欢迎贡献 (Issues / PR / Bug Report / Feature Request)
- 若社区变大，可添加 `CONTRIBUTING.md` / Issue /Pull Request 模板，方便协作

---

## ✅ 总结

`rust_video_downloader` 结合了 Rust 的性能、安全、跨平台 + GPUI 的 GPU 加速 /现代 GUI + 后端自动管理工具 + 用户友好界面，是一个非常有潜力、用户友好、对开发者也很合适的项目。这个 README / SPEC 已经覆盖了项目的整体架构、模块划分、功能设计、开发 & 发布流程、风险 & 注意事项，以及发展路线。

你可以在这个基础上继续扩展 (添加功能说明 /API 参考 /图示 /示例 /截图 /打包脚本 /CI/CD 流程等)，让它成为真正的项目文档。

---

如果你愿意，我也可以帮你把这个 **README + SPEC + TODO + Roadmap** 模板放进一个 **完整 zip**，结构与目录对应 (包括 `tool_manager`, `gui_app` skeleton) —— 作为 “项目启动骨架 (starter template)”。 要做这个吗？

[1]: https://longbridge.github.io/gpui-component/docs/installation?utm_source=chatgpt.com "Installation | GPUI Component"
[2]: https://github.com/longbridge/gpui-component?utm_source=chatgpt.com "GitHub - longbridge/gpui-component: Rust GUI components for building fantastic cross-platform desktop application by using GPUI."
[3]: https://docs.rs/gpui?utm_source=chatgpt.com "gpui - Rust"
[4]: https://crates.org.cn/crates/create-gpui-app?utm_source=chatgpt.com "create-gpui-app — Rust 的命令行工具 // Lib.rs · Rust 包仓库"
[5]: https://www.reddit.com//r/rust/comments/1ohe89l?utm_source=chatgpt.com "GitHub - longbridge/gpui-component: Rust GUI components for building fantastic cross-platform desktop application by using GPUI."

# MageKit AI 开发指南（重构后）

本文档用于在重构后的代码中快速定位核心入口与数据流，避免把旧的模块边界/流程带入当前实现。

更详细内容请参考：

- `readme.md`：产品介绍、构建/运行、面向用户的使用说明
- `docs/gui_app.md`：GUI（根 crate）入口与关键模块
- `crates/*/README.md`：各 crate 的职责与 API
- `CLAUDE.md`：仓库工作流与架构说明（偏工程化、含命令）

## 项目概述

MageKit 是一个基于 Rust 的跨平台桌面应用（GPUI），提供视频下载、任务队列、工具管理（yt-dlp/ffmpeg）、平台解析（自研 + yt-dlp 回退）、网页资源嗅探与直播录制能力。

## 技术栈

### 核心依赖

| 技术               | 版本         | 用途                          |
| ------------------ | ------------ | ----------------------------- |
| **Rust**           | 2024 edition | 主要编程语言                  |
| **GPUI**           | latest       | Zed 编辑器的 GUI 框架         |
| **gpui-component** | latest       | GPUI 组件库（按钮、输入框等） |
| **gpui-router**    | 0.2.7        | 页面路由系统                  |
| **yt-dlp**         | 外部工具     | 视频下载核心                  |
| **ffmpeg**         | 外部工具     | 视频/音频处理                 |
| **tokio**          | 1.0          | 异步运行时                    |
| **smol**           | 2            | 轻量级异步（与 GPUI 配合）    |

### Workspace crates（内置模块）

| crate | 主要职责 |
| --- | --- |
| `magekit-app` | GPUI UI、路由、`AppState`；只做“调度与展示” |
| `magekit-shared` | 共享类型/配置/常量/路径工具（`AppConfig`、`TaskStatus`、`DownloadOptions` 等） |
| `magekit-tool-manager` | 工具与任务统一管理层：队列/并发/持久化/事件广播 |
| `magekit-download` | 下载执行层（直链/yt-dlp/ffmpeg/HLS-DASH），提供进度事件与取消能力 |
| `magekit-extractor` | 平台解析层：抖音/TikTok 自研优先，其余平台优先走 yt-dlp |
| `magekit-bytedance` | 抖音 Web API 与签名（A-Bogus/X-Bogus），供解析与录制复用 |
| `magekit-capture` | 网页资源嗅探：静态扫描 + CDP 网络监听，事件流输出资源 |
| `live_recorder` | 直播录制：平台抽象、录制 handle、进度查询 |
| `xbogus` | X-Bogus/AB-Sign 签名实现 |

### 配置与工具目录

跨平台目录解析位于 `crates/shared/src/utils.rs`：

- 配置目录：`get_app_config_dir()` → `dirs::config_dir()/MageKit/`（主配置：`config.toml`）
- 数据目录：`get_app_data_dir()` → `dirs::data_dir()/MageKit/`
- 工具目录：`get_tools_dir()` → `<data_dir>/tools/`

常见路径示例（以 `dirs` 实际返回为准）：

| 平台 | 工具目录 |
| --- | --- |
| Windows | `C:/Users/<User>/AppData/Roaming/MageKit/tools/` |
| macOS | `~/Library/Application Support/MageKit/tools/` |
| Linux | `~/.local/share/MageKit/tools/` |

工具解析优先级（与 UI/ToolManager 的处理一致）：

1. 应用内安装（`magekit_tool_manager::ToolStorage`）
2. 应用 tools 目录 + 系统 PATH（`magekit_shared::resolve_yt_dlp_path()` / `resolve_ffmpeg_path()`）

## 项目结构

```
magekit-app/
├── Cargo.toml              # Workspace + 根 crate 配置
├── src/                    # GUI 主应用
│   ├── main.rs             # 应用入口
│   ├── app/                # 应用状态管理
│   ├── ui/                 # 页面与组件
│   └── theme/              # 主题相关
├── crates/                 # 其他库 crates
│   ├── shared/             # 共享类型/配置/路径工具
│   ├── tool_manager/       # 工具管理 + 任务队列/持久化/事件
│   ├── download/           # 下载执行层（直链/yt-dlp/ffmpeg/HLS-DASH）
│   ├── extractor/          # 平台解析层（自研 + yt-dlp 回退）
│   ├── bytedance/          # 抖音 API + 签名
│   ├── capture/            # CDP 嗅探库
│   ├── live_recorder/      # 直播录制/平台适配
│   └── xbogus/             # X-Bogus/AB-Sign 签名
├── themes/                 # 主题文件目录（运行时热加载，当前为 JSON）
├── assets/                 # 图片/资源
├── docs/                   # 文档与说明
├── crawlers/               # Python 爬虫/原型（未与 GUI 强耦合）
└── py_demo/                # Python demo/对照实现
```

## 关键入口（推荐阅读顺序）

- `src/main.rs`：Application 初始化（日志、主题热加载、router、打开主窗口）
- `src/ui/main_window.rs`：路由与页面缓存（home/tasks/tools/settings/channel/record/capture）
- `src/app/state.rs`：`AppState`（Tokio runtime、`ToolManager`、配置、任务缓存、事件通道）
- `src/app/download.rs` / `src/app/tools.rs` / `src/app/capture.rs`：GUI 调度包装（不承载核心业务）
- `crates/tool_manager/src/task_manager.rs`：任务队列/并发/持久化/事件广播的核心实现

## 运行项目

```bash
# 开发模式运行
cargo run --bin magekit

# 编译
cargo build

# Release 编译
cargo build --release --bin magekit
```

## GPUI 开发要点

### 组件模式

GPUI 使用类似 React 的组件模式，但基于 Rust 的所有权系统：

```rust
pub struct MyComponent {
    // 组件状态
    value: String,
}

impl MyComponent {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self { value: String::new() }
    }
}

impl Render for MyComponent {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .child(text(&self.value))
    }
}
```

### 重要概念

1. **Entity<T>** - 组件的句柄，类似 React 的 ref
2. **Context<Self>** - 组件上下文，用于状态更新和事件处理
3. **Window** - 窗口上下文
4. **cx.notify()** - 通知 GPUI 重新渲染组件
5. **cx.spawn()** - 在组件上下文中执行异步任务

### 事件处理

```rust
// 使用 listener
Button::new("btn")
    .label("Click me")
    .on_click(cx.listener(|this, _event, _window, cx| {
        this.handle_click(cx);
    }))

// 使用闭包
.on_click(move |_event, _window, _cx| {
    // 处理点击
})
```

### 样式系统

GPUI 使用链式调用的 Tailwind 风格样式：

```rust
div()
    .flex()                    // display: flex
    .flex_col()                // flex-direction: column
    .gap_2()                   // gap: 8px (0.5rem)
    .p_4()                     // padding: 16px
    .rounded_lg()              // border-radius: 8px
    .bg(theme.background)      // background-color
    .text_color(theme.foreground)
    .size_full()               // width: 100%; height: 100%
    .w(px(200.0))              // width: 200px
    .h_auto()                  // height: auto
```

### 异步操作

GPUI 不能直接 await，需要用 `cx.spawn`：

```rust
// ✅ 正确方式
cx.spawn(async move |this, cx| {
    let result = some_async_operation().await;
    let _ = this.update(cx, |this, cx| {
        this.data = result;
        cx.notify();
    });
}).detach();

// ✅ 使用 smol::unblock 执行阻塞操作
cx.spawn(async move |this, cx| {
    let result = smol::unblock(move || {
        // 阻塞操作
        expensive_computation()
    }).await;
    // ...
}).detach();
```

### 主题

主题文件位于 `themes/`，启动时通过 `gpui_component::ThemeRegistry::watch_dir(...)` 监听目录并热加载。

UI 使用 gpui-component 的主题系统：

```rust
let theme = cx.theme();
div()
    .bg(theme.background)
    .text_color(theme.foreground)
    .border_color(theme.border)
```

## 核心架构

### AppState（全局状态）

```rust
pub struct AppState {
    pub tool_manager: Arc<ToolManager>,      // 工具/任务统一入口（下载逻辑由 ToolManager 承载）
    pub config: Arc<RwLock<AppConfig>>,      // 应用配置
    pub tasks: Arc<RwLock<HashMap<TaskId, TaskStatus>>>, // 任务状态缓存（事件驱动更新，UI 侧主要只读）
    pub event_tx: mpsc::Sender<AppEvent>,    // 应用事件发送器（通知 UI）
    pub event_rx: mpsc::Receiver<AppEvent>,  // 应用事件接收器
    pub runtime: Arc<Runtime>,               // Tokio 运行时（提供给 GUI 层做异步桥接）
}
```

AppState 通过 `GlobalAppState` 在 GPUI 中全局共享。

### 下载流程

1. 用户输入 URL（HomePage）
2. `AppState::get_video_info_in_background(url)` → `ToolManager::get_video_info(...)`（解析由 `magekit-extractor` 负责）
3. 用户选择格式/输出目录/附加选项
4. `AppState::start_download_in_background_with_info(...)` → `ToolManager::start_download_with_info(...)`
5. `ToolManager` 内部调用 `magekit-download` 执行下载（按策略选择 yt-dlp/ffmpeg/直链/HLS-DASH 等）
6. `ToolManagerEvent`（broadcast）驱动 AppState 更新 `tasks` 缓存，UI 侧轮询/订阅刷新展示

### 任务控制

- 暂停/恢复/取消：通过 `AppState` 的同步包装方法委托给 `ToolManager`（`pause_download` / `resume_download` / `cancel_download`）
- 删除：先取消，再从持久化中删除任务状态（`delete_task_status`），并从 AppState 本地缓存移除

## 页面说明

### HomePage（首页）

- URL 输入框
- 视频预览（缩略图、标题、时长）
- 格式选择（视频格式、音频格式）
- 下载按钮

### TasksPage（任务页）

- 任务列表（下载中、已完成、失败）
- 任务操作（暂停、继续、取消、删除）
- 进度显示

### ToolsPage（工具页）

- yt-dlp 状态与更新
- ffmpeg 状态与更新
- 工具安装/卸载

### SettingsPage（设置页）

- 下载路径设置
- 主题选择
- 其他配置

### ChannelPage（频道页）

- 频道/用户主页信息获取（批量/订阅类场景）
- 支持将频道内视频加入下载队列

### RecordingPage（录制页）

- 直播录制入口（依赖 `live_recorder`）
- 展示录制状态/时长/输出路径等信息

### CapturePage（嗅探页）

- 网页资源嗅探（静态扫描 + CDP 网络监听）
- 捕捉到的资源可用于后续下载/转存（如 m3u8、媒体直链等）

## 常见问题

### GPUI 没有 overflow_visible

GPUI 目前不支持 `overflow: visible`，如果需要元素超出父容器，需要调整布局或使用绝对定位。

### 异步与 GPUI

- 不要在 render 中执行异步操作
- 使用 `cx.spawn()` 执行异步任务
- 使用 `smol::unblock()` 包装阻塞操作
- 使用 `this.update(cx, |this, cx| {...})` 更新组件状态

### 取消/暂停/恢复任务

统一通过 `AppState`/`ToolManager` 的任务控制 API：

- 暂停：`pause_download(...)`
- 恢复：`resume_download(...)`
- 取消：`cancel_download(...)`

UI 层不要直接管理子进程；取消/清理由下载执行层在内部完成（CancellationToken + 资源清理，必要时终止外部进程）。

## 代码风格

- 使用中文注释
- 使用 emoji 在日志中标记状态（🚀 启动，✅ 成功，❌ 失败，⚠️ 警告）
- 模块使用 `mod.rs` + 子模块文件的结构
- 类型定义集中在 `types.rs`
- 工具函数集中在 `utils.rs`

## 调试

```bash
# 查看详细日志
RUST_LOG=debug cargo run --bin magekit

# 只看关键模块日志
RUST_LOG=magekit_tool_manager=debug,magekit_download=debug cargo run --bin magekit
```

日志会输出到终端，包括：

- yt-dlp 的 stdout/stderr
- 下载进度解析结果
- 任务状态变化

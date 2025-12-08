# MageKit AI 开发指南

本文档旨在帮助 AI 助手理解 MageKit 项目的架构、技术栈和开发规范。

## 项目概述

MageKit 是一个基于 Rust 的跨平台视频下载器应用，使用 GPUI 框架构建原生 GUI。核心功能是通过 yt-dlp 下载各种网站的视频。

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

### 外部工具路径

工具路径由 `crates/shared/src/utils.rs` 中的函数动态获取：

| 平台        | 路径                                             |
| ----------- | ------------------------------------------------ |
| **macOS**   | `~/Library/Application Support/MageKit/tools/`   |
| **Windows** | `C:\Users\<User>\AppData\Roaming\MageKit\tools\` |
| **Linux**   | `~/.local/share/MageKit/tools/`                  |

相关函数：

- `get_app_data_dir()` - 应用数据目录
- `get_tools_dir()` - 工具目录
- `get_app_config_dir()` - 配置目录（使用 `dirs::config_dir()`）

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
│   ├── shared/             # 共享类型（VideoInfo, TaskStatus 等）
│   ├── tool_manager/       # 工具管理/下载器
│   ├── live_recorder/      # 直播录制/平台适配
│   └── xbogus/             # X-Bogus/AB-Sign 签名
├── themes/                 # 主题文件目录
└── docs/                   # 文档与说明
```

## 运行项目

```bash
# 开发模式运行
cargo run

# 编译
cargo build

# Release 编译
cargo build --release
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

使用 gpui-component 的主题系统：

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
    pub tool_manager: Arc<ToolManager>,     // 工具管理器
    pub config: Arc<RwLock<AppConfig>>,     // 应用配置
    pub tasks: Arc<RwLock<HashMap<TaskId, TaskStatus>>>,  // 任务列表
    pub event_tx: mpsc::Sender<AppEvent>,   // 事件发送器
    pub event_rx: mpsc::Receiver<AppEvent>, // 事件接收器
    pub runtime: Arc<Runtime>,              // Tokio 运行时
    pub download_cancel_flags: Arc<Mutex<HashMap<TaskId, Arc<AtomicBool>>>>, // 取消标志
}
```

AppState 通过 `GlobalAppState` 在 GPUI 中全局共享。

### 下载流程

1. 用户输入 URL
2. 调用 `tool_manager.get_video_info(url)` 获取视频信息
3. 用户选择格式/画质
4. 调用 `app_state.download_video_in_background()` 开始下载
5. 下载在后台线程执行，通过进度回调更新 UI
6. 下载完成后更新任务状态

### yt-dlp 调用

```rust
let mut cmd = std::process::Command::new(&yt_dlp_path);
cmd.arg(&url)
   .arg("--format").arg(&format_id)
   .arg("--output").arg(&output_template)
   .arg("--newline")      // 逐行输出进度
   .arg("--progress")
   .arg("--no-mtime");

// 获取视频信息
cmd.arg("-j")  // JSON 输出
   .arg("--flat-playlist");  // 不展开播放列表
```

### 进度解析

yt-dlp 输出格式：

```
[download]  45.2% of ~12.34MiB at 1.23MiB/s ETA 00:05
```

使用正则/字符串解析提取进度、速度、大小。

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

## 常见问题

### GPUI 没有 overflow_visible

GPUI 目前不支持 `overflow: visible`，如果需要元素超出父容器，需要调整布局或使用绝对定位。

### 异步与 GPUI

- 不要在 render 中执行异步操作
- 使用 `cx.spawn()` 执行异步任务
- 使用 `smol::unblock()` 包装阻塞操作
- 使用 `this.update(cx, |this, cx| {...})` 更新组件状态

### 取消下载

下载使用子进程执行 yt-dlp，取消需要：

1. 设置取消标志
2. 在主循环中检测标志
3. 调用 `child.kill()` 终止进程

## 代码风格

- 使用中文注释
- 使用 emoji 在日志中标记状态（🚀 启动，✅ 成功，❌ 失败，⚠️ 警告）
- 模块使用 `mod.rs` + 子模块文件的结构
- 类型定义集中在 `types.rs`
- 工具函数集中在 `utils.rs`

## 调试

```bash
# 查看详细日志
RUST_LOG=debug cargo run

# 只看 magekit 相关日志
RUST_LOG=magekit=debug cargo run
```

日志会输出到终端，包括：

- yt-dlp 的 stdout/stderr
- 下载进度解析结果
- 任务状态变化

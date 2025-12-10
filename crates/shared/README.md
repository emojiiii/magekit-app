# magekit-shared

共享类型、常量与工具函数库，为 UI、下载管理器、录制器等模块提供统一的数据结构。

## 模块

- `types::download`：`VideoFormat`、`VideoInfo`、`DownloadOptions`、`DownloadParams`
- `types::task`：`TaskId`、`TaskState`、`TaskStatus`、`TaskUpdate`
- `types::config`：`AppConfig`、`DownloadConfig`、`ToolsConfig`、`UiConfig`、`AdvancedConfig`
- `types::platform`：`PlatformCookie`、`ChannelInfo`、`LiveRecordConfig`、`MonitoredRoom` 等
- `types::event`：`AppEvent`、`Notification`、`ToolUpdateEvent`
- `constants`：默认 UA、域名、路径模板等常量
- `utils`：格式化、校验、路径工具；工具解析函数在 `tools` feature 下提供

## 快速上手

```rust
use magekit_shared::{
    AppConfig, DownloadOptions, TaskStatus, TaskId,
    format_file_size, validate_url, load_app_config_or_default,
};

// 默认下载选项与格式化
let opts = DownloadOptions::default();
println!("输出模板: {:?}", opts.output_template);
println!("1MB = {}", format_file_size(1024 * 1024));

// 任务状态
let task = TaskStatus::new(TaskId::new_v4(), "https://xx".into(), None);
assert!(task.is_active());

// Serde 读写配置
let cfg = load_app_config_or_default();
let toml = toml::to_string_pretty(&cfg)?;
let parsed: AppConfig = toml::from_str(&toml)?;

// URL 校验
let url = validate_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ")?;
println!("host = {:?}", url.host_str());
```

## Feature

- `tools`（默认开启）：启用工具解析与无窗口 Command (`resolve_yt_dlp_path`/`resolve_ffmpeg_path`/`resolve_browser_path`、`create_command`、`create_tokio_command`)，并拉入可选依赖 `which`、`tokio`。

## 优点

- 按领域拆分的模块使类型发现与 IDE 导航更友好。
- 统一的数据模型减少跨 crate 重复定义，便于序列化与持久化。
- 默认值完备（下载目录、格式、字幕语言等），可直接用于 UI 表单与 CLI。
- 工具路径/配置工具函数集中，跨平台路径一致；可通过 feature 关闭工具相关依赖。

## 局限 / 风险

- 配置/任务字段多，仍需结合业务场景选择性展示。
- 直播相关类型依赖 `chrono`，在极简场景可仅启用 `types::download/task/config`。

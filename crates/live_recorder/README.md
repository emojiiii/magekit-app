# Live Recorder

提供 CLI / 库两种直播录制方式：抖音继续使用稳定的 Rust 原生录制流程，其他平台交给 Streamlink 插件。

## 支持的平台

- 抖音 URL 使用原有 Rust 原生录制器。
- Bilibili、斗鱼、虎牙、SOOP 等其他平台由当前 Streamlink 版本内置插件决定。
- 快手及没有 Streamlink 插件的平台不受支持；Streamlink 失败时不会回退到旧平台处理器。
- 旧的 `platforms/*` 处理器仍保留为独立兼容模块，`LiveRecorder` 仅对抖音调用原生录制器。

## 核心 API

- `LiveRecorder` / `LiveRecorderCore`
  - `new()` / `with_soop_credentials(username, password)`
  - `start_recording(url, RecordConfig)` → `RecordingHandle`
  - `check_room_status(url)`、`get_stream_info(url)`
  - 便捷录制：`quick_record(url, output_template)`
- `RecordConfig`
  - `output_path_template`、`quality: VideoQuality`、`format`
  - `max_duration`、`proxy`、`headers` 等
- `RecordingHandle`
  - `get_progress()`、`wait()`、`stop()`（见 `recorder.rs`）
- SOOP 认证
  - 公开房间通常可匿名访问；遇到登录限制时可配置 Global Cookie（平台名 `soop_global`）或调用 `with_soop_credentials`。
  - 19+ 房间仍要求已完成成人认证且具备房间观看权限的账号。

## 快速开始

### 作为库

```rust
use live_recorder::{LiveRecorder, RecordConfig, VideoQuality};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let recorder = LiveRecorder::new();
    let mut handle = recorder
        .start_recording(
            "https://live.douyin.com/123456",
            RecordConfig {
                output_path_template: "./downloads/{platform}_{timestamp}.mp4".into(),
                quality: VideoQuality::High,
                format: "mp4".into(),
                ..Default::default()
            },
        )
        .await?;

    while let Some(p) = handle.get_progress().await {
        println!("录制中: {:.1?}s", p.duration);
    }
    handle.wait().await?;
    Ok(())
}
```

### 命令行

```bash
cargo run -- --url https://live.douyin.com/123456 \
  --output ./my_video.mp4 \
  --quality hd \
  --proxy http://127.0.0.1:7890
```

## 路径模板变量

`{platform}` / `{anchor_name}` / `{room_id}` / `{title}` / `{timestamp}` / `{quality}`

示例：`./downloads/{platform}/{anchor_name}/{room_id}_{timestamp}.mp4`

## 错误与依赖

- 错误类型：`RecorderError`（房间不存在、流不可用、认证失败、限流等）
- 抖音沿用原生录制流程；其他平台由 Streamlink 插件取流，媒体管线使用 FFmpeg `-c copy`。

## 平台支持与边界

- 抖音的解析、状态检查、取流和录制均走原生处理器；其他平台由 Streamlink 解析和录制，支持范围随 Streamlink 版本变化。
- 快手及没有 Streamlink 插件的平台不受支持，也不会回退到其他原生处理器。
- 仓库仍保留旧的 `platforms/*` 公开模块，供现有调用方兼容；`LiveRecorder` 只对抖音走原生录制器。
- Streamlink 后端暂不支持 `segment_duration` 和弹幕保存；抖音仍按原生录制器现有行为处理。

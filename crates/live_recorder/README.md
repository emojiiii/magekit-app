# Live Recorder

可扩展的直播录制工具，支持多平台直播流录制并提供 CLI / 库两种使用方式。

## 支持的平台

- ✅ 抖音
- ✅ Bilibili
- ✅ 斗鱼 / 虎牙 / 快手 / SOOP（对应 `platforms/*` 实现）
- 🚧 其余平台可通过新增 `PlatformHandler` 扩展

## 核心 API

- `LiveRecorder` / `LiveRecorderCore`
  - `new()` / `with_factory(PlatformFactory)`
  - `start_recording(url, RecordConfig)` → `RecordingHandle`
  - `check_room_status(url)`、`get_stream_info(url)`
  - 便捷录制：`quick_record(url, output_template)`
- `RecordConfig`
  - `output_path_template`、`quality: VideoQuality`、`format`
  - `max_duration`、`proxy`、`headers` 等
- `RecordingHandle`
  - `get_progress()`、`wait()`、`stop()`（见 `recorder.rs`）
- 扩展接口
  - 实现 `platforms::PlatformHandler` 可接入新平台

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
- 主要依赖：`tokio`、`reqwest`、`serde`、`xbogus`、`tracing`、`clap`

## 扩展新平台

实现 `PlatformHandler` trait，声明 `platform_name`、`supported_url_patterns`，并实现 `extract_room_id` / `get_stream_info`，即可通过 `PlatformFactory` 注册。

## 优点

- 平台处理抽象清晰，可独立扩展新平台。
- 支持代理、质量选择、路径模板，覆盖常见录制需求。
- 异步流式下载，进度可订阅，CLI/库复用同一核心。

## 局限 / 风险

- 录制流程仍依赖外部 ffmpeg，缺少启动前的可用性检测。
- 各平台实现成熟度不一致，缺少集成测试保障。
- `RecordConfig` 部分字段缺少文档/示例（如自定义 headers）。

## 改进建议

- 在启动录制前检查 ffmpeg、输出目录可写性，并给出友好错误。
- 为主流平台添加端到端测试与回归用例，避免协议变更导致静默失败。
- 增加 WebSocket/事件流接口，便于 UI 实时显示录制状态与错误。

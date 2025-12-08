# Live Recorder

一个可扩展的直播录制工具，支持多个平台的直播流录制。

## 特性

- 🎯 **多平台支持**: 可扩展的架构，轻松添加新的直播平台
- 🔧 **灵活配置**: 支持多种录制质量和格式
- 🚀 **高性能**: 异步IO，支持流式下载
- 📦 **易于使用**: 简单的API和命令行工具
- 🔐 **支持代理**: 内置代理支持
- 📊 **进度监控**: 实时录制进度和状态

## 支持的平台

- ✅ **抖音** (Douyin)
- 🚧 **更多平台** (架构已准备就绪，可轻松扩展)

## 快速开始

### 安装依赖

```bash
# 需要安装 FFmpeg (用于 MP4 格式录制)
# Ubuntu/Debian:
sudo apt-get install ffmpeg

# macOS:
brew install ffmpeg

# Windows:
# 下载 FFmpeg 并添加到 PATH
```

### 基本使用

#### 作为库使用

```rust
use live_recorder::{LiveRecorder, RecordConfig, VideoQuality};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建录制器
    let recorder = LiveRecorder::new();

    // 抖音直播间URL
    let url = "https://live.douyin.com/123456";

    // 配置录制参数
    let config = RecordConfig {
        output_path_template: "./downloads/{platform}_{anchor_name}_{timestamp}.mp4".to_string(),
        quality: VideoQuality::High,
        format: "mp4".to_string(),
        ..Default::default()
    };

    // 开始录制
    let mut handle = recorder.start_recording(url, config).await?;

    // 监控录制进度
    while let Some(progress) = handle.get_progress().await {
        println!("录制中... 时长: {}秒", progress.duration);
        if progress.status == live_recorder::RecordStatus::Completed {
            break;
        }
    }

    // 等待录制完成
    handle.wait().await?;
    Ok(())
}
```

#### 命令行使用

```bash
# 基本录制
cargo run -- --url https://live.douyin.com/123456

# 指定输出路径和质量
cargo run -- --url https://live.douyin.com/123456 \
  --output ./my_video.mp4 \
  --quality hd

# 使用代理
cargo run -- --url https://live.douyin.com/123456 \
  --proxy http://127.0.0.1:7890

# 仅检查直播间状态
cargo run -- --url https://live.douyin.com/123456 --check
```

## API 文档

### LiveRecorder

主要的录制器类，提供以下方法：

- `new()` - 创建新的录制器
- `start_recording(url, config)` - 开始录制
- `check_room_status(url)` - 检查直播间状态
- `get_stream_info(url)` - 获取流信息

### RecordConfig

录制配置：

```rust
pub struct RecordConfig {
    pub output_path_template: String,    // 输出路径模板
    pub quality: VideoQuality,           // 视频质量
    pub format: String,                  // 录制格式 (mp4, flv, m3u8)
    pub max_duration: Option<u64>,       // 最大录制时长
    pub proxy: Option<String>,           // 代理设置
    pub headers: HashMap<String, String>, // 请求头
    // ... 其他字段
}
```

### VideoQuality

支持的视频质量：

- `Original` / `Blue` - 原画/蓝光
- `Ultra` - 超清
- `High` - 高清
- `Standard` - 标清
- `Low` - 流畅

## 扩展新平台

要添加新的直播平台支持，只需实现 `PlatformHandler` trait：

```rust
use async_trait::async_trait;
use live_recorder::platforms::PlatformHandler;

pub struct NewPlatformHandler;

#[async_trait]
impl PlatformHandler for NewPlatformHandler {
    fn platform_name(&self) -> &'static str {
        "new_platform"
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec!["newplatform.com", "live.newplatform.com"]
    }

    async fn extract_room_id(&self, url: &str) -> RecorderResult<String> {
        // 实现房间ID提取逻辑
        todo!()
    }

    async fn get_stream_info(&self, room_id: &str) -> RecorderResult<StreamInfo> {
        // 实现流信息获取逻辑
        todo!()
    }
}
```

## 路径模板变量

输出路径模板支持以下变量：

- `{platform}` - 平台名称
- `{anchor_name}` - 主播名称
- `{room_id}` - 房间ID
- `{title}` - 直播标题
- `{timestamp}` - 时间戳
- `{quality}` - 视频质量

示例：
```
./downloads/{platform}/{anchor_name}/{room_id}_{timestamp}.mp4
```

## 错误处理

库使用 `RecorderError` 统一处理各种错误情况：

- `RoomNotFound` - 房间不存在
- `StreamNotAvailable` - 流不可用
- `AuthenticationFailed` - 认证失败
- `RateLimitExceeded` - 触发频率限制
- 等等...

## 依赖项

- `tokio` - 异步运行时
- `reqwest` - HTTP客户端
- `serde` - 序列化/反序列化
- `xbogus` - X-Bogus签名生成
- `tracing` - 日志记录
- `clap` - 命令行参数解析

## 许可证

MIT License

## 贡献

欢迎提交 Issue 和 Pull Request！

## 注意事项

1. 请确保遵守各平台的服务条款
2. 录制功能仅用于个人学习和研究
3. 请勿用于商业用途或侵犯版权
4. 使用代理时请确保网络连接稳定
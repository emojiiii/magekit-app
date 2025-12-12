# MageKit Capture

通用的资源捕捉库，支持从网页中捕捉各种类型的资源（视频、音频、图片等）。

## 功能特性

- **静态扫描**：快速扫描 HTML 内容查找资源链接
- **浏览器 CDP**：通过 Chrome DevTools Protocol 实时监听网络请求
- **资源筛选**：支持按类型（视频/音频/图片/文档等）筛选资源
- **事件驱动**：通过事件流实时推送发现的资源

## 使用示例

### 基本使用

```rust
use magekit_capture::{CaptureRequest, ResourceFilter, start_capture};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let request = CaptureRequest {
        target_url: "https://example.com".to_string(),
        custom_browser_path: None,
        headless: true,
        timeout: Duration::from_secs(30),
        filter: ResourceFilter::all(), // 捕捉所有资源
        auto_speedup_ads: false, // 不自动加速广告
        speedup_rate: 2.0,
    };

    let mut session = start_capture(request).await?;

    // 注意：捕捉会持续运行，不会自动结束
    // 需要手动调用 session.cancel() 来停止
    while let Some(event) = session.rx.recv().await {
        match event {
            CaptureEvent::Found(resource) => {
                println!("发现资源: {} ({:?})", resource.url, resource.resource_type);
            }
            CaptureEvent::Log(msg) => {
                println!("日志: {}", msg);
            }
            CaptureEvent::Finished => {
                println!("捕捉完成（通常不会自动触发）");
                break;
            }
            CaptureEvent::Error(err) => {
                eprintln!("错误: {}", err);
            }
        }
    }

    Ok(())
}
```

### 只捕捉视频资源，并自动加速播放广告

```rust
let request = CaptureRequest {
    target_url: "https://example.com".to_string(),
    custom_browser_path: None,
    headless: true,
    timeout: Duration::from_secs(30),
    filter: ResourceFilter::video_only(),
    auto_speedup_ads: true, // 启用自动加速广告
    speedup_rate: 2.0, // 2倍速播放（可设置 1.0-16.0）
};
```

当检测到广告时，会自动通过 CDP 控制浏览器中的视频播放速度，快速跳过广告。

### 只捕捉音频和视频

```rust
let request = CaptureRequest {
    target_url: "https://example.com".to_string(),
    custom_browser_path: None,
    headless: true,
    timeout: Duration::from_secs(30),
    filter: ResourceFilter::media_only(),
};
```

### 只捕捉图片

```rust
let request = CaptureRequest {
    target_url: "https://example.com".to_string(),
    custom_browser_path: None,
    headless: true,
    timeout: Duration::from_secs(30),
    filter: ResourceFilter::image_only(),
};
```

### 自定义筛选器

```rust
let filter = ResourceFilter::only_types(&[
    ResourceType::Video,
    ResourceType::Audio,
])
.exclude_pattern("ad".to_string())  // 排除包含 "ad" 的 URL
.min_size(1024 * 1024)  // 最小 1MB
.max_size(100 * 1024 * 1024);  // 最大 100MB

let request = CaptureRequest {
    target_url: "https://example.com".to_string(),
    custom_browser_path: None,
    headless: true,
    timeout: Duration::from_secs(30),
    filter,
};
```

### 取消捕捉任务

```rust
// 在另一个任务中
session.cancel();
```

## 资源类型

支持以下资源类型：

- `Video` - 视频资源（mp4, m3u8, flv, webm 等）
- `Audio` - 音频资源（mp3, m4a, aac, ogg 等）
- `Image` - 图片资源（jpg, png, gif, webp 等）
- `Document` - 文档资源（pdf, doc, txt 等）
- `Font` - 字体资源（woff, ttf, otf 等）
- `Stylesheet` - 样式表（css）
- `Script` - JavaScript 文件
- `Other` - 其他类型

## 架构

- `types.rs` - 类型定义（资源类型、事件、请求等）
- `filter.rs` - 资源筛选器
- `scanner.rs` - 静态扫描器
- `cdp.rs` - Chrome DevTools Protocol 监听器
- `core.rs` - 核心捕捉逻辑

## 注意事项

1. **持续运行**：捕捉任务会持续运行，不会自动结束。需要手动调用 `session.cancel()` 来停止
2. **浏览器路径**：如果未指定 `custom_browser_path`，将自动检测系统中的 Chrome/Chromium 浏览器
3. **无头模式**：`headless=true` 时浏览器在后台运行，不会显示窗口
4. **超时设置**：`timeout` 仅用于静态扫描的超时时间，CDP 监听会持续运行直到手动取消
5. **资源去重**：自动过滤重复的 URL
6. **广告检测**：自动检测广告资源（基于 URL 关键词、域名、文件大小等），检测到广告时可选择自动加速播放
7. **自动加速**：`auto_speedup_ads=true` 时，检测到广告会自动通过 CDP 控制视频播放速度（`speedup_rate`，范围 1.0-16.0）

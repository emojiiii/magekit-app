# magekit-extractor

媒体解析器，负责根据 URL 自动选择平台解析策略。对抖音 / TikTok 使用自研解析，其他平台优先使用 yt-dlp，并向上层返回统一的 `VideoInfo` / `ChannelInfo`。

## 核心 API

- `MediaExtractor::new(yt_dlp_path: PathBuf)` 创建解析器（需指定 yt-dlp 路径）
- `supported_platforms()` 返回已支持的平台列表
- `get_video_info(url, cookies)` 自动检测平台并返回视频信息
- `get_channel_info(url, cookies)` 获取频道 / 用户主页信息（抖音走自研，其余走 yt-dlp）

输入 Cookie 类型为 `magekit_shared::PlatformCookie`，可选传入以处理登录态/地区限制。

## 简单示例

```rust
use magekit_extractor::MediaExtractor;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let extractor = MediaExtractor::new(PathBuf::from("/path/to/yt-dlp"));
    let info = extractor.get_video_info("https://www.douyin.com/video/xxx", None).await?;
    println!("标题: {}", info.title);
    Ok(())
}
```

## 优点

- 统一入口，隐藏平台差异，对 UI 与下载层透明。
- 自研抖音/TikTok 解析优先，失败时回退 yt-dlp，减少失败率。
- 支持频道信息获取，便于批量下载/订阅场景。

## 局限 / 风险

- 平台检测与分发逻辑较简单，边缘 URL 可能判定不准。
- 抖音/TikTok 解析依赖 Cookie 与签名，缺少清晰的错误分类与提示。
- yt-dlp 路径需要外部传入，未做存在性检查或版本校验。

## 改进建议

- 抽象 `PlatformResolver` trait，允许为新平台添加解析插件并可配置优先级。
- 增加平台检测的单元测试与 URL 正则覆盖，减少误判。
- 对 yt-dlp 执行增加健康检查（版本、可执行性），失败时回退提示。
- 将 Cookie / UA / 代理作为构造参数或上下文，便于调用方统一管理。


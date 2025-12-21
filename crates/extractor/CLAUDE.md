# CLAUDE - extractor

Breadcrumb: Home / extractor

## 角色

- 平台无关的媒体解析层：优先自研 Douyin/TikTok/Bilibili，其余平台回退 yt-dlp；为 `tool_manager::VideoDownloader` 提供统一的 `VideoInfo/ChannelInfo/ChannelPageResult`。

## 关键文件

- `src/lib.rs`：`MediaExtractor` 入口，持有 yt-dlp 路径，暴露 `supported_platforms`、`get_video_info`、`get_channel_info`、`get_channel_page`（按 URL 自动分派，自研 → yt-dlp）。
- `src/platform.rs`：平台枚举与 URL 识别，处理短链域名兜底。
- `src/douyin.rs`：基于 `platform_api::DouyinApi` 解析抖音视频与用户主页；短链解析、aweme_id/sec_user_id 提取；频道页分页抓取作品。
- `src/tiktok.rs`：TikTok 自研解析（X-Bogus 签名 + Web API）。
- `src/bilibili.rs`：Bilibili 自研解析：视频详情/播放地址（progressive 直链）与 UP 投稿分页（WBI）。
- `src/ytdlp.rs`：yt-dlp 桥接；`--dump-json/--flat-playlist` 获取视频/频道，支持平台定向 Cookie header，支持分页（YouTube 通过 `--playlist-items`）。
- `src/cookies.rs`：从 `PlatformCookie` 构造请求 Cookie；`error.rs` 定义 `ExtractError/Result`。

## 数据流与要点

- `MediaExtractor::get_video_info`：检测平台 → Douyin/TikTok/Bilibili 自研 → 其他平台交给 yt-dlp。
- `get_channel_info/get_channel_page`：Douyin/Bilibili 走自研分页接口；YouTube 等平台走 yt-dlp（并在 UI 层按 Tab 独立分页缓存）。
- Cookie 选取逻辑：按平台首个启用的 cookie；未命中则不附带，可能导致受限内容失败。

## 依赖

- `magekit-shared` 类型与工具、`platform-api` 多平台 API、自带 `regex/serde/tokio/reqwest/anyhow` 等；dev 依赖 `which` 检测工具。

## 注意事项

- yt-dlp 路径必须可执行；命令失败会直接返回 stderr。
- 自研解析依赖网络与 Cookie（不同平台要求不同），建议 UI 显示失败原因。
- 若后续扩平台：先补 `Platform::detect/all_supported`，再实现对应解析器或 yt-dlp 映射。

## 推荐下一步

- 为 Bilibili/Douyin/TikTok 自研解析补齐更稳定的“离线”单测向量；网络集成测试继续保持 `#[ignore]`，避免 CI 误报。

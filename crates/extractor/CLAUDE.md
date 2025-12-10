# CLAUDE - extractor

Breadcrumb: Home / extractor

## 角色

- 平台无关的媒体解析层，优先自研抖音/TikTok，其他平台回退 yt-dlp；为 `tool_manager::VideoDownloader` 提供 `VideoInfo/ChannelInfo`。

## 关键文件

- `src/lib.rs`：`MediaExtractor` 入口，持有 yt-dlp 路径，暴露 `supported_platforms`、`get_video_info`、`get_channel_info`（按 URL 自动分派，自研 → yt-dlp）。
- `src/platform.rs`：平台枚举与 URL 识别，处理短链域名兜底。
- `src/douyin.rs`：基于 `bytedance::DouyinApi` 解析抖音视频与用户主页；短链解析、aweme_id/sec_user_id 提取，格式转换为 `VideoInfo/ChannelInfo`，分页抓取作品。
- `src/tiktok.rs`：TikTok 自研解析（未细读，需确认 Cookie/签名要求）。
- `src/ytdlp.rs`：yt-dlp 桥接；`--dump-json/--flat-playlist` 获取视频/频道，支持平台定向 Cookie header，缺失 entries 时逐行解析 fallback。
- `src/cookies.rs`：从 `PlatformCookie` 构造请求 Cookie；`error.rs` 定义 `ExtractError/Result`。

## 数据流与要点

- `MediaExtractor::get_video_info`：检测平台 → Douyin/TikTok 自研 → 其他平台交给 yt-dlp，失败返回 `ExtractError::CommandFailed/Parse`。
- `get_channel_info`：抖音用户主页走自研（分页 50 页上限）；其余平台使用 yt-dlp `--flat-playlist` 并合并 tab/entries。
- Cookie 选取逻辑：按平台首个启用的 cookie；未命中则不附带，可能导致受限内容失败。
- yt-dlp 调用使用 `create_tokio_command`，需可执行的 yt-dlp 路径；B 站附带 `--extractor-args BiliBiliSpace:metadata=true`。

## 依赖

- `magekit-shared` 类型与工具、`bytedance` 抖音 API、自带 `regex/serde/tokio/reqwest/anyhow` 等；dev 依赖 `which` 检测工具。

## 注意事项

- 需要有效的 yt-dlp 路径；命令失败会直接返回 stderr。
- 抖音/TikTok 解析依赖网络与 Cookie，建议 UI 显示失败原因；长列表分页存在 50 页硬上限。
- 若后续扩平台，先补 `Platform::detect/all_supported`，再实现对应解析器/yt-dlp 映射。

## 推荐下一步

- 细读/补测 `tiktok.rs` 与 `ytdlp.rs` 异常路径、Cookie 覆盖；为自研解析增加单元/集成测试（含无 Cookie/失效 Cookie 场景）。

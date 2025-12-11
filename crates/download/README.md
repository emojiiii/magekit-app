# download crate 技术文档

面向场景：提供统一的“下载”能力（直链/yt-dlp/ffmpeg/自定义下载器），上层如 `tool_manager` 只负责任务编排与工具路径管理。

## 目标

- 给定 URL + 参数，选择合适的下载实现并执行。
- 支持自定义 headers/cookie/query/外部工具额外参数。
- 提供进度、日志、完成、失败回调；可取消。
- 可插拔下载器，默认内置 direct/yt-dlp/ffmpeg。

## 模块

```
src/
  lib.rs             // 对外 re-export
  config.rs          // DownloadRequest/Output/Strategy/Extra/Retry
  progress.rs        // 进度、事件、回调与 channel helper
  error.rs           // DownloadError/Result
  dispatcher.rs      // DownloadClient，选择并调用下载器
  downloader/
    mod.rs           // Downloader trait & registry
    direct.rs        // 直链下载（占位）
    ytdlp.rs         // yt-dlp 下载（占位）
    ffmpeg.rs        // ffmpeg 下载（占位）
    hls_dash.rs      // HLS/DASH 下载（占位）
  utils.rs           // 公共工具（占位）
```

## 核心类型

- `DownloadRequest`：url、strategy、output、extra(headers/cookie/query/tool_args)、timeout、bandwidth_limit、retries、resume。
- `DownloadStrategy`：`Auto | Direct | YtDlp | Ffmpeg | Custom(String)`。
  - 已预留 `HlsDash` 变体，可直接指定使用 HLS/DASH 插件。
- `DownloadOutput`：directory + template。
- `DownloadExtra`：headers、cookie、query、`tool_args`（按下载器名称透传）。
- `DownloadCallback`：同步回调（要求轻量），通常在实现里把事件发到 channel。
- `ChannelCallback`：把事件投递到 `UnboundedSender<DownloadEvent>`。
- `DownloadEvent`：`Progress | Complete | Error | Log`。
- `DownloadOutcome`：输出路径、content_type、details。

## 回调模型

- trait 同步方法，避免在下载器内部 await UI 逻辑；在回调内部做 O(1) 投递到 channel。
- UI 侧异步消费事件，更新状态/界面；取消由上层通过 `CancellationToken` 触发。

## 下载器插件

- 实现 `Downloader` trait 并注册到 `DownloaderRegistry`。
- `DownloadClient::with_defaults()` 会注册 direct/yt-dlp/ffmpeg，占位实现可逐步替换为真实逻辑。

## 外部工具参数

- `DownloadExtra.tool_args`：`HashMap<String, Vec<String>>`，key 为下载器名称（如 `"ytdlp"`、`"ffmpeg"`）。
- headers/cookie/query 透传到各下载器；外部进程需要转换为对应 CLI（后续在实现中处理）。

## 取消与重试

- `CancellationToken` 在 download 链路透传，下载器需要定期检查并清理资源。
- `RetryPolicy`（默认 2 次，2s backoff），可由上层自行循环重试。

## TODO / 状态

- direct：已实现 Range 续传、临时文件 .part -> rename、平滑速度（5s 窗口）。
- yt-dlp：已实现命令组装、stdout 进度解析、stderr 收集、Destination 输出文件解析、进程退出码映射。
- ffmpeg：已实现 headers/cookie 注入、stderr size/time 解析估速、stderr 收集、退出码映射。
- hls_dash：已实现 m3u8 解析、最高带宽选择、并发分段下载、AES-128 解密、mpd 走 ffmpeg fallback。
- 待补：安全（外部工具 args 白名单/转义；输出路径清理）；测试（进度解析单测；集成测试、本地 http server / mock 进程）；DASH/mpd 原生并发与校验。

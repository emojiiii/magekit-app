# 调研记录

## 当前任务

用户希望测试 Streamlink，暂时关闭旧的原生录制入口，最好通过系统设置控制“是否仅启用 Streamlink”。

## 待确认

- 设置配置类型及默认值位置
- 系统设置页面的控件模式
- 录制页入口与启动调度的分流位置
- 是否已有 Streamlink-only 或运行时选择逻辑

## 已确认

- `LiveRecordConfig` 位于 `crates/shared/src/types/platform.rs`，由 `AppConfig.live_record` 持久化。
- `LiveRecorder` 当前支持 `Auto`、`Streamlink`、`Native` 三种后端；`LiveRecorder::new()` 只读取环境变量，默认是 `Auto`。
- `Auto` 在 Streamlink 报告“不支持平台”时会回退到 `legacy_recorder`，这正是启动后无法直观看出后端的原因。
- 录制页在 `src/ui/pages/record/page.rs` 中直接创建 `LiveRecorder::new()`，可改为依据 `config.live_record` 选择后端。
- 系统设置页已有“高级设置”卡片和保存链路，适合增加“仅使用 Streamlink”开关。
- 旧原生录制实现仍由 `crates/live_recorder/src/legacy_recorder.rs` 保留；本次更适合关闭调用入口而不是删除代码，避免破坏兼容性和测试工厂。
- 录制页保存房间/录制配置时会整体写回 `live_record`，因此保存逻辑需要显式保留系统设置中的 `streamlink_only`，避免缓存页面覆盖该开关。

## 新一轮日志定位

- 用户日志中的“录制任务已启动”只发生在 `start_recording_with_cookies` 成功拿到句柄后，并不代表已收到媒体数据。
- Streamlink worker 的 `record()` 将 pull 子进程和两个 FFmpeg 管道的 stderr 全部重定向到 `DEVNULL`；SOOP 插件失败时只能返回泛化错误，日志看不到真实原因。
- `RecordingHandle::get_progress()` 在 Streamlink worker 通道关闭时返回 `None`；录制页把 `None` 当成正常完成，清除错误并标记任务完成，导致失败被掩盖。
- 录制页启动监控任务时立即调用一次 `check_all_rooms`，但该函数按房间顺序等待每次最多 90 秒的 Streamlink probe；第一个房间卡住/失败会延迟后续房间的首次刷新。
- 还需要检查已安装 Streamlink 版本中的 SOOP 插件对 `play.sooplive.co.kr/<id>/<broad_no>` 的解析和请求头要求，不能只依赖当前泛化错误。
- 已用应用实际安装的 Streamlink `8.6.1` 对用户日志中的 URL 做 probe：插件能匹配为 `soop`，当前接口返回 `360p/540p/720p/1080p` 等流，说明失败点不在 URL 解析或 Streamlink 基础 probe。
- worker 的 `record()` 对 `pull` 子进程退出码 13 没有立即输出原因，而是按重试预算继续重连；约 1+2+4 秒后才输出泛化失败，和用户约 8 秒后通道关闭的时间吻合。
- 当前 UI 即使收到最终 `status=error` 事件，也只更新字节数；下一次 `get_progress()` 返回 `None` 时会把任务清成“完成”，因此需要修正终态处理。
- 直接运行项目当前 worker（同一 Streamlink 8.6.1、同一 SOOP URL、同一 FFmpeg）复现：probe 成功，但 record 连续 30 秒只有 `connecting`、输出大小为 0，最终返回 `finished(status=error, message="No media was recorded")`。
- 因此 SOOP 的主要失败点是 `stream.open()`/HLS 首段读取没有产出数据；需要检查 HLS URL 请求/响应及 Streamlink 的请求头，而不是只改 UI。
- 进一步检查显示 Streamlink 8.6.1 返回的 SOOP HLS 播放列表来自 `live-global-cdn-v02.sooplive.com`，HTTP 200、约 1007 字节且包含媒体片段；但 worker 仍在 30 秒内没有读到任何字节，下一步要验证片段请求状态和是否全部被 `preloading` 过滤。
- 手工拉取同一个 HLS 播放列表的首个 TS 片段返回 HTTP 200、约 1.8 MB；直接调用 `stream.open().read(188)` 也能拿到 MPEG-TS 起始字节。因此 Streamlink/HLS 本身可读，问题集中在 worker 的子进程/FFmpeg 管道或其错误处理。
- 直接把 worker 以 `mode=pull` 作为子进程运行时，35 秒内 stdout 仍为 0 字节（无论用 `-c` 内嵌源码还是脚本路径），与直接在同一 Python 环境调用 Streamlink 不同；需要在 worker 内部区分卡在 `open()` 还是 `read()`。
- 已修复：录制启动复用首次 probe 返回的 SOOP HLS 地址，避免录制子进程再次调用 SOOP 插件的 `streams()`；直接 HLS 拉流同时补齐 `Referer`/`Origin`。
- 已修复：录制 worker 的安全 stderr 摘要会传回父进程，录制页等待 worker 最终状态后再结算，不再把进度通道关闭当成成功完成。
- 已修复：首次房间状态检查改为并发执行，并在请求开始时显示 `Checking`，一个慢平台不会阻塞其它房间。

## 启动闪退与失效封面

- 用户日志中的封面请求包含 Douyin `403`、Huya/SOOP `404`；这些 URL 来自直播间缓存或平台返回的动态封面，不应被视为录制失败。
- `RecordingPage::new` 启动时会把 `cached_cover_url` 直接放入 GPUI `img()`；失效的历史 URL 会在首次布局前集中触发资源加载。
- GPUI 的 `Img` 支持 `.with_fallback(...)`，当前录制页、频道页和首页预览均未设置失败占位。
- 录制页刷新成功但 `cover_url=None` 时，现有逻辑保留旧的运行时封面和持久化封面；这会使失效 URL 持续被重试。
- 需要让启动状态先使用占位图，待本次状态刷新拿到新封面后再显示；刷新得到 `None` 时同步清理缓存。
- 已修复：录制页启动恢复 `cached_cover_url`；状态刷新拿到新封面时替换，返回 `None` 或失败时保留历史图过渡。
- 已修复：录制页、频道页和首页预览的远程图片均增加 `with_fallback`，资源请求失败只显示占位，不再把错误图片节点交给后续布局。
- 回归结果：重新启动并完成两轮 21 个房间状态检查，没有再出现封面资源错误或 Taffy panic。

## 历史封面过渡修正

- 用户需要历史封面作为启动阶段的即时展示，不能因为防止过期 URL 而完全禁用缓存。
- 正确策略是恢复 `cached_cover_url` 的初始渲染；成功拿到新 `cover_url` 时替换；刷新失败或暂时没有新封面时保留旧值。
- 远程图片失败由 `Img::with_fallback` 替换成固定占位元素，因此过期缓存只影响图片显示，不应进入异常布局状态。
- 已修正实现：启动恢复历史 URL；成功刷新时替换运行时和持久化值；刷新失败/无新封面时保留历史值。
- 回归结果：当前版本启动并完成房间状态刷新，无 Taffy panic；编译、测试和格式检查全部通过。

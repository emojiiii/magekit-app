# 调研记录

## 2026-09-23：SOOP 画质与连接延迟

- `문채원♡` 当前频道元数据返回 `sd/360p`、`hd/540p`、`hd4k/720p`、`original/1080p`；设置为 `Original`。现存自动 remux 的 MP4 是 1280×720，`-c copy` 不会改变分辨率。历史任务没有记录实际 preset，不能断定当时是降档重试还是设置曾改变。
- 当前 worker 原画独立拉流选中 `original`，23 秒收到首批媒体，输出约 25 MB、1920×1080 TS。原来的零数据重试会自动把第二次尝试改为排序第二档，可能静默得到 720p；已改为原画只重试原画，并记录所选标签。
- 分阶段诊断显示频道元数据约 7–12 秒、播放授权及流地址约 8–13 秒、首个 HLS 片段约 8 秒；远端请求占主导。短期复用房态元数据可省去重复频道查询，但一次有界实录仍在第 23 秒收到媒体，网络波动抵消了部分收益。
- 手动录制房间在房态刷新后后台预热所选流的授权和地址，房态显示不等待预热。预热完成后同房间流对象选取近乎即时，首个 TS 包约 7.9 秒到达；实际有界录制第 9 秒收到媒体，写出约 23 MB、1920×1080 TS，日志确认 `original [预热流]`，临时文件已清理。预热信息只在内存保留 90 秒；失效时重新授权。

## 2026-09-23：SOOP 韩文路径录制误入乱码目录

- 本次 GUI 运行的 debug 可执行文件和托管 Python worker 均为最新版本；正确的 `신서율♥` 目录仅有 0 字节占位文件，同一时间戳的乱码目录曾有 33,310,968 字节 TS，另一个旧时间戳有 25,447,868 字节 TS。两个文件均为有效 188 字节同步包；用户随后明确表示已自行删除这些文件。
- Rust 通过管道发送 UTF-8 JSON，Windows 托管 Python 的重定向标准输入默认编码是 GBK。`sys.stdin.readline()` 按 GBK 解码后，将 `신서율♥` 精确变成磁盘上的乱码目录名。因此先前测试在临时 ASCII 路径成功，遗漏了 GUI 的韩文实际路径。
- worker 改为从 `sys.stdin.buffer` 显式按 UTF-8 读取请求；Rust 首次收到正数字节进度时核对自己预期路径的真实文件长度，避免再次把写入别处的字节显示为 `REC`。
- worker 的 daemon 控制线程使用缓冲 stdin 读取时，终态后可能触发 Python `_enter_buffered_busy` 致命退出；已改为原始 `os.read`，并让 Rust 收到终态后关闭 stdin。无效地址的定向验证现在返回正常错误码 13，无崩溃。
- 修复后用用户当前配置检查该 SOOP 房间，平台返回 Offline；因此当前无法做同房间新的有效媒体实录。含韩文路径的本地 worker 验证确认输出文件生成在正确目录，之后临时文件已清理。

## 2026-09-23：SOOP 零字节录制

- 用户新日志中录制句柄约 45 毫秒即启动，说明前置全档解析耗时已消除；之后约 44 秒仅出现“进度监控停止”。该日志只说明 UI 房间状态不再标记录制，无法代表 worker 正常完成。
- 本机当前没有 magekit 进程，09:15 的 SOOP 目标目录已为空；录制器在后台错误且文件长度为零时会删除空占位文件。
- 有界诊断中对同一房间直接启动 `pull`，25 秒内收到约 1.5 MB 且以 TS 同步字节 `0x47` 开头，证明该房间当前的单档授权与源流可读；正在检查 `record_direct_ts` 子进程到输出文件的搬运与停止时序。
- 第二次有界 `pull` 诊断等待 22 秒仍未收到首字节，而 10 秒 `record_direct_ts` 诊断也为 0 字节；同一源流的首包到达时间存在显著波动，不能仅归因于录制父进程的搬运。
- 更细的阶段计时：同一房间 Original 档位解析约 0.28 秒、选档授权约 13.47 秒、`stream.open()` 基本即时、首个 188 字节约在启动后 20.89 秒到达。当前 `record_direct_ts` 从启动子进程起按约 30 秒 `last_data` 超时；当授权和首片请求稍慢，父进程可能在首包前中断拉流并再次从头授权，造成长时间零字节。
- `record_direct_ts` 在首字节到达前仍可能每次等待至少 30 秒并按 retry_count 重试，连接中状态可能持续很长时间；SOOP 授权解析的 20 秒限制不覆盖 HLS `open/read`。
- 修复后按子进程的选档/打开阶段计首包等待，并以无缓冲管道读取后立即写入 TS。首选档零数据时最多降一档重试，总首包等待有上限。
- 有界实录第 29 秒开始收到媒体，35 秒结束时 TS 为 1,564,348 字节且任务正常完成；同一房间的首包速度仍受网络与 SOOP 源站波动影响。

## 2026-09-23：SOOP 启动等待与工具页卡片位置

- GUI 传给录制器的 `output_path_template` 已经是生成好的绝对目标路径，不含 `{anchor_name}` 等模板变量；因此 SOOP GUI 录制可以安全跳过启动前的完整 probe，同时保留其它调用方/模板路径的旧预检行为。
- 当前 Streamlink 8.6.1 SOOP 插件 `_get_streams()` 会对 `VIEWPRESET` 的每个清晰度分别取 HLS 授权 key 和 CDN 播放地址；启动前 probe 与录制 pull 都可能重复执行全档解析。录制只需按用户选择解析一个档位，可以直接调用插件已有 `_get_hls_stream()` 保留 `aid` 授权参数。
- 工具页的外层包装组件把 Deno 和 Streamlink 卡片放在基础工具页之前，基础页面自己的滚动容器随后显示 yt-dlp、FFmpeg 和“关于工具”。应将运行时控件作为基础 ToolsPage 的插槽，在标准工具卡片之后、提示卡之前渲染，避免独立滚动/页面区域。

## 2026-09-23：抖音封面、SOOP 检查速度与代理

- 本机 `config.toml` 当前 `advanced.proxy` 为未启用；Windows 系统代理可被 Python 的系统代理发现接口识别为本机 `127.0.0.1:7890`。此前 Streamlink 会话显式设置 `http-trust-env=false`，且录制器没有把 `advanced.proxy` 传给 worker，因此系统代理不会自动用于 SOOP。
- 配置里部分抖音封面仍是 `p11-webcast-sign.douyinpic.com` 的远程 URL；通过 Windows 系统代理读取其中一张图片返回 HTTP 200/JPEG。GPUI 图片节点直接取远程 URL，不会复用 Python/reqwest 代理配置；改由录制层通过选定代理下载到 MageKit 本地封面缓存。
- 抖音状态接口在离线分支会提前返回，且不带直播封面；当前从用户头像字段补图，直播封面优先级不变。本地缓存只在首次/手动封面读取时请求，正常状态轮询沿用缓存。
- Streamlink 8.6.1 的 SOOP `_get_streams()` 在取得频道信息后，会按 `VIEWPRESET` 逐档调用 HLS key 与 CDN 地址接口。状态轮询无需逐档解析；新路径只读取广播号和频道状态/主播元数据，遇到登录限制仍返回认证错误，并将 SOOP 状态请求超时设为 10 秒。录制启动仍使用完整流解析路径。

## 2026-09-23：Windows Streamlink worker 命令长度

- 错误 `IO error: 文件名或扩展名太长。 (os error 206)` 在多种 Streamlink 平台同时出现，而抖音仍可录制；这与它们共用 Streamlink worker、抖音走原生 worker 的执行路径相吻合。
- `streamlink_worker.py` 源码约 36,242 个字符。旧启动命令把整份源码放在 `python -c <source>` 参数中；Windows `CreateProcess` 命令行长度上限约 32,767 个 UTF-16 字符，因此创建 probe/record worker 时会返回错误 206。
- 只缩短 Rust 启动命令还不够：Python worker 在录制中还会再次 `python -c` 启动 pull 子进程。现统一从受管环境中的 worker 脚本文件按路径启动。
- worker 文件名包含源码摘要；写入采用临时文件 + rename，并校验已存在内容，防止多个应用实例读到半写入文件。

## 2026-09-23：封面、录制页卡顿与应用图标

- `crates/live_recorder/src/streamlink_worker.py::probe` 目前将所有 Streamlink 平台的 `cover_url` 固定为 `None`；Douyin 原生实现单独解析封面，所以截图呈现“只有抖音有封面”是数据源缺失，不是 GPUI 图片组件问题。
- Streamlink probe 已创建带平台 Cookie 的 `Streamlink` HTTP 会话。界面以前直接请求平台 CDN 图片，仓库已有 403/404 记录；现改为在该会话中取 Open Graph / Twitter 图并下载到 `<data_dir>/MageKit/live_covers/`，GPUI 从本地图片文件读取。状态轮询只对本次运行尚未查过的房间取一次封面，手动刷新仍会重新取图。
- `RecordingPage::check_all_rooms` 每轮原先为全部房间各启动一个 Tokio 探测任务，且慢检查会与下一轮重叠；活动房间还会每个轮询周期同步写一次配置文件。现改用最多 4 路并发、单轮互斥，仅首次发现开播时更新持久化时间，并将轮询产生的缓存异步写盘。
- 当前仓库有 `assets/favicon.ico`，但发布工作流引用的 `assets/icon.png` / `assets/icon.icns` 均不存在；Windows 发布也没有向 EXE 嵌入图标资源，因此文件和任务栏显示系统通用图标。
- 已用 imagegen 生成高对比的深蓝底、青紫工具箱播放符号图稿，输出为 1024px PNG、256px PNG、多尺寸 ICO 和 ICNS；Windows RC 资源编译后已从最终 EXE 成功提取到 32px 图标。
- 现有构建脚本只在 `crates/live_recorder/build.rs`；根 GUI crate 尚无 `build.rs`。发布工作流已经为 macOS 和 Linux预留 `assets/icon.icns` 与 `assets/icon.png` 的引用位。

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

## 2026-09-23：SOOP 韩国 Cookie 与虎牙 probe

- Streamlink 8.6.1 的 SOOP 插件即使匹配 `play.sooplive.co.kr`，也通过 `https://afevent2.sooplive.com/api/get_private_info.php` 检查登录，并向 `https://live.sooplive.com/afreeca/player_live_api.php` 查询频道数据。
- 原实现将 `sooplive` Cookie 只绑定到 `sooplive.co.kr`，且 Rust 只会在韩国房间 URL 下传递它；插件 `.com` API 因而看不到已配置的韩国 Cookie。
- 修复时让明确选择的 SOOP 韩国 Cookie 同时可用于 SOOP 官方 `.com` API；如果用户另选了 SOOP Global Cookie，Global Cookie 仍优先用于 `.com`，韩国 Cookie 继续保留在 `.co.kr` 房间域。
- SOOP 插件会在收到 Cookie 但 `_check_auth()` 失败时记录明确日志；worker 现在把此情形与房间要求登录/成年认证的提示区分开。
- 虎牙 UI 的 `Invalid Streamlink probe response` 来自 Rust 对 worker stdout 的整块 JSON 解析失败。worker 是 JSON Lines 协议；Rust 现在逐行找协议事件，并在没有事件时只暴露退出码、stdout 长度和 Python 异常类型，不回显 stderr 原文、Cookie 或 URL。

## 2026-09-23：SOOP 名称/录制与虎牙封面

- 添加直播间探测失败时，`add_room` 把主播名写成“获取失败”；之后周期 `check_all_rooms` 成功只更新状态、标题和封面，未更新主播名。手动 `refresh_room` 也只覆盖 `Unknown`，因此成功日志与卡片占位名可以同时存在。
- 当前 Streamlink worker 的 SOOP record 路径没有接收并复用 probe 返回的 HLS URL，录制阶段会重新 resolve/请求 SOOP 插件；工作树相对原有实现还缺少 HLS 直连和 TS 直写路径。恢复后可避免第二次插件解析，并让 TS 录制不经过双层 FFmpeg 管道。
- 虎牙房间 HTML 没有 `og:image`/`twitter:image` 元数据，因此通用页面元数据提取对该平台取不到封面。虎牙现有原生 handler 使用 `mp.huya.com/cache.php` 的 `liveData.screenshot`/头像字段；对截图房间号 920611 的公开接口读取返回截图，图片 CDN 返回 JPEG 200。
- 录制卡片原先在拿到 RecordingHandle 后马上显示 `REC` 并从点击时计时；连接阶段即便文件字节数仍为 0，也会看起来正在录制。现在只有收到输出字节后才累计时长/显示 REC，等待媒体时显示“连接中”。

# 进度记录

## 2026-09-23：SOOP 原画与预热（完成）

- 核对 `문채원♡` 当前可用档位、成品 MP4 与直播源：原画实际提供 1920×1080，历史成品为 1280×720。
- 原画录制不再在首包慢时自动换成 720p；进度日志显示实际 preset。SOOP 房态查询复用频道元数据，并对手动录制后台预热播放授权。
- 当前房间做了两次有界实录：未预热首媒体约 23 秒、约 25 MB 1080p；预热后首媒体约 9 秒、约 23 MB 1080p。两次任务正常完成并清理了临时文件。
- Python AST、`cargo fmt --all`、`cargo check --locked --bin magekit`、`git diff --check` 与 `cargo build --locked --bin magekit` 均通过；未运行测试套件。

## 2026-09-23：SOOP 韩文路径修复（完成，待房间开播后实录复核）

- 核对当前运行程序和 worker 版本，找到同时间戳的乱码目录非空 TS；恢复副本与源文件哈希一致，之后用户表示已自行删除这些文件。
- worker 输入协议改为显式 UTF-8，停止线程改为原始管道读取；Rust 加入目标路径真实字节校验和终态后关闭 stdin；录制页异步进度绑定具体任务 ID。
- 定向复现并消除了 worker 终态时的 Python 致命退出；当前房间状态检查返回 Offline，无法进行在线媒体实录复核。
- Python AST、`cargo fmt --all`、`git diff --check`、`cargo check --locked --bin magekit` 与 `cargo build --locked --bin magekit` 通过。

## 2026-09-23：SOOP 零字节录制（完成）

- 已确认最新日志显示启动耗时已降至毫秒级，但录制文件未接收到媒体字节；当前输出目录为空。
- 已在后台录制异常、用户请求停止、停止失败及零字节停止路径加入明确日志，便于区分 worker 失败和手动停止。
- 有界 SOOP pull 诊断收到约 1.5 MB TS；进一步定位到首包超时此前把授权耗时算在内，且父进程缓冲读取可能延迟小批量媒体的落盘。
- worker 改为在打开流后计算首包超时、无缓冲读取并即时写盘；无数据时最多降一档重试，并限制整次首包等待。
- 有界实际录制于第 29 秒收到媒体，35 秒结束时生成 1,564,348 字节 TS，正常完成；临时验证文件已清理。
- `cargo fmt --all`、Python AST 解析、`git diff --check`、`cargo check --locked --bin magekit` 和 `cargo build --locked --bin magekit` 均通过；未运行测试套件。

## 2026-09-23：SOOP 启动等待与工具页布局（完成）

- 已确认录制等待来自 `start_recording_with_cookies` 在返回 handle 前调用全量 probe；Streamlink SOOP 插件对每个清晰度都会发送独立授权/API 请求。
- 已确认 GUI 提供的录制路径为完整绝对文件名；非 GUI 模板路径仍需主播信息，因此本轮只对 SOOP + 完整绝对路径跳过前置 probe。
- 已确认运行时卡片位于基础 ToolsPage 的独立滚动容器外；现改为基础页面的 runtime controls 插槽，布局放在 yt-dlp/FFmpeg 卡片之后、“关于工具”之前。
- SOOP 在录制页提供完整绝对输出路径时跳过启动前的全量 probe；其它平台及使用模板路径的调用仍保留状态 probe。
- SOOP worker 根据所选质量直接调用插件的频道/授权/播放地址接口，只解析目标档位并最多尝试一个相邻档，保留插件生成的 `aid` 参数；SOOP API 授权请求总预算为 20 秒，超时立即报告，不做多轮重连。
- Deno 与 Streamlink 管理控件现在嵌入基础工具页同一滚动容器，在 yt-dlp/FFmpeg 卡片后、“关于工具”前以双列紧凑卡片显示。
- `cargo fmt --all`、`cargo check --locked --bin magekit`、Python AST 语法解析、`git diff --check` 和 `cargo build --locked --bin magekit` 通过；未运行测试套件，也未启动应用/执行实际直播录制。

## 2026-09-23：恢复抖音封面并优化 SOOP 与系统代理

- 抖音封面解析增加离线主播头像回退；封面/头像通过当前设置代理缓存为本地文件，后续状态轮询复用本地图片。
- SOOP 房态检查改用频道状态元数据接口，不解析全部 HLS 清晰度，不在轮询时抓取直播页封面；状态请求超时缩短到 10 秒。录制仍走完整流解析。
- 将设置页代理配置接入直播录制。系统代理模式读取环境变量或 Windows 当前用户 Internet Settings；Streamlink worker 不再绕过设置代理。
- 已确认本机 Windows 系统代理可发现为 `127.0.0.1:7890`，但 MageKit 当前保存的代理模式为未启用；需在设置中选择“系统代理”后，新版录制器才会使用它。
- `cargo fmt --all -- --check`、`cargo check --locked --bin magekit`、Python AST 语法检查、`git diff --check`、`cargo build --locked --bin magekit` 均通过；未运行测试套件。

## 2026-09-23：修复 Streamlink 多平台 `os error 206`

- 已定位截图错误源于 Windows 创建 Streamlink worker 子进程失败：旧代码将约 36K 字符的 Python 源码内嵌进 `python -c` 参数。
- 已改为按 worker 源码摘要在受管 Python 环境中生成脚本文件，并用短脚本路径启动 probe、record 和 pull 子进程。
- `cargo fmt --all`、`cargo check --locked --bin magekit`、Python AST 语法解析、`git diff --check` 和 `cargo build --locked --bin magekit` 均通过；未运行测试套件。

## 2026-09-23：封面、录制页卡顿与应用图标

- 已定位 Streamlink worker 把 `cover_url` 固定为 `None`，确认这是抖音独有封面的原因。
- 已为 Streamlink probe 增加可选的直播页分享图提取，复用平台 Cookie 会话，并将图片写入本地缓存供 GPUI 使用；封面读取失败不会影响直播状态/流解析。
- 录制页只对尚未尝试过的房间获取一次分享封面；人工刷新仍强制更新。
- 录制页状态轮询改为最多 4 个房间并发，上一轮未完成时跳过新轮次；只在首次发现开播时更新时间，并将状态缓存写盘移出 UI 回调。
- 已生成 `assets/icon.png`、`assets/icon-256.png`、`assets/icon.ico`、`assets/icon.icns`，并接入 Windows EXE 与 Linux AppImage 构建。
- `cargo fmt --all -- --check`、`cargo check --locked --bin magekit`、Python 语法检查、`git diff --check` 全部通过；`cargo build --locked --bin magekit` 成功，并从 EXE 提取到 32px 内嵌图标。未运行测试套件。

## 2026-09-23

- 已读取项目工作区说明和 planning-with-files 技能要求。
- 已创建任务计划、调研记录和进度记录。
- 已确认 `Auto` 后端会在 Streamlink 不支持时回退到旧原生录制，设置页已有可复用的高级设置开关组件。
- 已增加 `LiveRecordConfig.streamlink_only`，旧配置缺少该字段时默认开启。
- 已在系统设置“高级”区域增加“仅使用 Streamlink 录制”开关，并接入配置保存。
- 录制页现在按最新配置动态创建 `Streamlink` 或 `Auto` 后端，并显示当前引擎；默认不会回退到原生录制。
- `cargo check --workspace --all-targets --locked` 已通过且无 warning。
- 已修正录制页保存逻辑，避免缓存的旧录制配置覆盖 Streamlink-only 开关。
- 全工作区测试通过：7 + 6 + 9 + 1 + 5 + 18 + 14 + 10 = 70 项；格式检查和 `git diff --check` 通过。
- 任务完成。

## 2026-09-23：SOOP 录制失败回归

- 收到用户日志：任务句柄显示已启动，但约 8 秒后进度通道关闭；SOOP 录制始终不成功。
- 同时收到启动后首次直播状态刷新不显示的问题。
- 已定位到 worker stderr 被丢弃、通道关闭被当作成功完成，以及首次检查按房间串行等待的风险。
- 已读取应用实际安装的 Streamlink 8.6.1 SOOP 插件，并确认首次 probe 能返回可用 HLS 地址。
- 已实现 SOOP 直连 HLS、请求头补齐、worker 错误摘要回传、录制终态等待，以及并发首次房间状态刷新。
- 已确认实时管道的双层 FFmpeg 在直播未结束时会等待 EOF；SOOP 的 TS 直链现在直接写入 `.ts` 文件，避免该缓冲问题。
- 实际 SOOP worker 回归通过：25 秒测试产生约 23.3 MB，最终状态为 `completed`，FFmpeg 可读取生成的 TS。
- `cargo check --workspace --all-targets --locked` 通过且无新增 warning；全工作区 lib 测试通过（70 项），`cargo fmt` 与 `git diff --check` 通过。
- 已清理本轮生成的临时 SOOP 测试文件。

## 2026-09-23：启动闪退与失效封面

- 收到用户新增日志：远程封面 403/404 后，Taffy 在布局解析阶段 panic，应用直接退出。
- 已确认录制页启动会恢复历史封面 URL，且多处远程图片没有失败占位；正在修复缓存初始化、缓存清理和图片 fallback。
- 已完成：启动恢复历史封面 URL；拿到新封面时替换；所有远程图片增加失败占位。
- 已完成回归：应用连续完成两轮房间检查，无图片资源错误、无 Taffy panic；`cargo check --workspace --all-targets --locked` 无 warning，全工作区 70 个 lib 测试通过，格式和 diff 检查通过。

## 2026-09-23：恢复历史封面过渡

- 用户反馈历史封面不能完全禁用，需要先显示缓存、再用最新封面替换。
- 已确认 GPUI fallback 会在图片资源失败时渲染替代元素，适合保留历史 URL 并处理过期封面。
- 已完成：恢复历史封面初始化；只用新非空封面替换缓存；失败/无新封面时保留历史图，由 fallback 显示占位。
- 已完成回归：应用启动并完成房间刷新，无 Taffy panic；`cargo check`、70 个 lib 测试、格式检查和 diff 检查通过。

## 2026-09-23：SOOP 韩国 Cookie 与虎牙探测响应

- 对照本机 Streamlink 8.6.1 的 `soop.py` 确认认证检查域是 `.sooplive.com`，SOOP 韩区 Cookie 原来只进入 `.sooplive.co.kr`，插件 API 因此无法校验它。
- 已修正 Rust Cookie 过滤及 Python Cookie jar 注入：韩区 Cookie 可用于韩国房间域和 SOOP 官方 `.com` API；有单独 Global Cookie 时保持 Global 优先。
- Cookie 设置页选中“SOOP 韩国”时提示该域名行为；worker 报告 Cookie 已送达但校验失败时给出区分提示。
- 虎牙 worker probe 按 JSON Lines 解析协议事件；无有效事件时给出退出码、输出长度及异常类型，隐藏原始 stderr/URL/Cookie。
- `cargo check --locked --bin magekit`、`cargo fmt --all -- --check`、Python 源码语法检查、`git diff --check` 均通过；未运行测试套件。
- 尝试生成新的 `target/debug/magekit.exe` 时发现 PID 30000 仍在运行并锁定旧 exe，Windows 拒绝替换；没有终止该进程。新代码已通过 `cargo check`，关闭 MageKit 后即可重新构建运行版本。

## 2026-09-23：SOOP 名称/录制与虎牙封面

- 修正自动轮询和手动刷新成功后回填“获取中/获取失败/Unknown”主播名，避免卡片长期保留旧占位文本。
- SOOP 录制复用 probe 已拿到的 HLS URL；pull worker 使用 SOOP 插件 HLS stream 类并带上正确的 Referer/Origin；TS 格式恢复直写媒体流，跳过双层 FFmpeg 管道。
- 虎牙封面先查 `mp.huya.com/cache.php` 的直播截图字段，再回退通用 meta 标签；限制图片 CDN 域名为 Huya/MSStatic，并沿用本地缓存。公开房间 API 和图片 CDN 已确认返回截图/JPEG。
- 录制进度在输出文件出现媒体字节前显示“连接中”，字节到达后才开始显示 REC 计时。
- `cargo fmt --all`、`cargo check --locked --bin magekit`、Python 语法检查、`git diff --check` 和 `cargo build --locked --bin magekit` 均通过；未运行测试套件。

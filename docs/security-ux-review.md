# MageKit 代码、安全与交互审查（2026-09-30）

## 范围与结论

本次基于 GPUI Fast / GPUI Kit 迁移提交 `958adf3`，审查实际下载、任务、频道、录制、嗅探、设置和工具更新路径。沿用 GPUI Kit 的 Button、Checkbox、Alert/AlertDialog、Empty、Spinner、Progress、Tag 和主题色；没有引入另一套设计系统。保留中文、英文和跟随系统语言。

这是一次有针对性的源码审查和修复，不是完整渗透测试、依赖漏洞认证或安全保证。没有使用真实账号 Cookie、安装线上更新、下载受限内容或访问用户电脑。Linux 交互使用隔离配置和离线工具夹具；Windows/macOS 检查由同一迁移 PR 的 GitHub Actions 运行，结果以对应提交为准。

## 已修复的问题与复现线索

| 编号 | 严重性 | 原始问题 / 复现 | 修复位置与验证 |
| --- | --- | --- | --- |
| S1 | 高 | 未识别 URL 的平台变成空字符串，`contains("")` 选中第一个启用 Cookie；URL 路径/查询/用户名中的平台域名也被误认 | `shared/utils.rs`、`extractor/{cookies,platform}.rs`、`tool_manager`：解析 HTTP(S) host，精确域名边界和 Cookie 平台匹配。覆盖恶意子域、userinfo、路径、查询、空值和控制字符测试 |
| S2 | 高 | 嗅探启动执行 `pkill chrome.*--headless` / Windows 同类命令，可能终止不属于本应用的浏览器；同时强制关闭 sandbox、站点隔离和跨站 Cookie 安全策略 | `capture/core.rs`：每次使用私有临时 profile，只持有并停止本次 child/CDP task；保留浏览器安全默认值，调试绑定 loopback。参数回归测试禁止危险标志 |
| S3 | 高（本地共享临时目录攻击）/ 中（更新完整性） | yt-dlp 使用可预测共享暂存路径、不校验下载摘要、忽略更新通道，替换跨文件系统易失败 | `tool_manager/updater.rs`：唯一私有同目录 staging、安装串行、限定大小/超时、同一官方 release tag 的准确 SHA-256 条目校验、原子替换；失败保留旧工具，Nightly 使用官方 nightly 仓库 |
| S4 | 中 | 下载和嗅探 info 日志包含 Cookie、Authorization 或完整 ffmpeg header 块；外部工具 header 可包含 CRLF | 删除相关值日志；共享 URL redactor 去除 userinfo/query/fragment；所有内置下载 header 验证名称和控制字符。注意下文仍有日志残余风险 |
| S5 | 中 | 输出模板 `../`、绝对路径、Windows drive/UNC 和不可信扩展名可越过预期目录 | 下载边界拒绝跨目录模板与非法扩展名；保留合法相对嵌套模板以及 API 明确指定完整路径的语义 |
| S6 | 中 | 直链 HTTP 头或 body 停滞时取消无效；yt-dlp stderr 管道可能塞满死锁；取消后 Auto fallback 继续工作 | 对 send/body/final wait 做取消 select；分别处理 stdout/stderr EOF；终止自有进程、取消不重试 fallback。localhost 和假进程回归覆盖停滞及大 stderr |
| S7 | 中 | Resume 对未经确认的 206 直接追加，服务器忽略 Range 后未重新验证 HTML；自动探测可能读完整媒体 | 验证 Content-Range/最终长度，200 重启重新验证内容，探测限制 2 KiB；错误续传保留已有 part 文件 |
| S8 | 中 | 旧任务回调可覆盖暂停/取消/新任务；删除先移除内存再落盘；Clear completed 连失败和取消一起删；Retry 使用无人消费的旧队列 | 每次执行带 run identity；重复 Resume/Retry 在锁内防重；删除先持久化；只清理确认时完成记录快照；Retry 与 Resume 使用真实 semaphore executor，支持重启后的失败记录。新/持久化失败任务都经本地 HTTP 夹具重试至 Completed |
| S9 | 中 | config/history 直接 truncate 写入，故障可破坏旧文件；默认权限可能暴露明文凭据 | 私有临时文件、flush/sync、原子替换，Unix 私有权限测试；失败保留旧文件。损坏任务历史不再自动被空文件覆盖 |
| U1 | 中 | Home 解析完成后覆盖新输入；失败 enqueue 丢失并提前跳转；频道旧分页会回写新频道/页签 | 请求/导航 generation；输入变化和 Cancel 使旧结果失效；任务创建等待结果；失败保留预览和可重试选择；回调不抢走其他页面导航 |
| U2 | 中 | 频道批量下载发起线程即提示全部成功，失败条目丢失；任务按钮乐观更新状态掩盖错误 | 串行有界提交、Stop adding、成功才取消选中、报告部分失败；任务等待操作结果、pending 防重复；取消/删除/清理使用 Kit 确认对话框 |
| U3 | 中 | 嗅探 Stop → Restart 后旧启动或旧事件复活；错误只进不可见日志；同一资源可重复 enqueue | 会话代际隔离、启动包含取消范围、可见 Alert、资源状态枚举和处理入口防重、Enter 提交；停滞启动可在测试 1 秒上限内取消 |
| U4 | 中 | 设置异步快照乱序覆盖；保存失败仍声称成功；无人消费的有界事件队列使 Saving 永久不结束 | 每次保存尝试的代际校验（含验证失败），锁内合并最新配置，失败回滚/可见重试，非阻塞可选通知；录制后台写入不再覆盖新语言/主题/账号设置 |
| U5 | 低 / 中 | 代理用冒号拆分，IPv6/认证/端口错误；TCP 连通被称为完整代理成功；其他开关会保存尚未确认的账号输入 | URL parser、DNS+TCP 总超时、防重复/过期结果；明确“端口可达，未验证认证和转发”；账号只由“保存账号”提交，Cookie 默认遮罩 |

## UI 和架构整理

- Home 用 Kit 的空态、加载、错误、可换行格式按钮和默认选择代替大段手绘控制；选视频时合理搭配音频，取消保留输入，Retry 真正重试
- Tasks 的错误、取消、完成分类更明确，Failed 有 Retry；进度值使用 Kit 的 clamp/无确定总量状态，动作在窄宽度换行，删除说明文件会保留
- Channel 的页签、全选/单选使用 Kit；“全选已加载”与批量进度说明实际作用范围
- Capture 的输入在会话中锁定，Start/Stop/错误/空态与资源提交状态一致，页面支持纵向空间不足时滚动
- Record 删除有确认，非法地址保留对话框以便纠正，按真实 host 分类；合法未知站点仍交给 Streamlink，保留 Twitch 等既有支持
- Tools 使用 Kit Progress/Alert 和主题状态色，不依赖硬编码深色错误块；Settings 使用可重试保存错误提示
- 保留 AppState → ToolManager → 下载器边界；把域名、header、日志 URL、原子私有写入集中到 shared，避免在 UI 重写下载核心。没有进行未经验证的大规模 crate 拆分

## 测试与验收

本轮本地云 Linux 已运行：

- `cargo check --workspace --all-targets --locked`
- `cargo build --bin magekit --locked`
- `cargo test --workspace --locked`：155 passed、0 failed、11 ignored，包括 doctests（网络/账号集成按原有 ignore 保留）
- `cargo fmt --all -- --check`、`git diff --check`、`python3 scripts/check_i18n.py`（651 双语项）
- `cargo clippy --workspace --all-targets --locked`：成功完成，但仓库仍存在较多既有风格/复杂度 warnings；没有将 warning 当成“零问题”
- 独立复查曾发现 Retry 死队列/持久化失败重试、录制站点白名单缩窄、设置旧回调清错和可选事件队列阻塞；已修正，新增回归防止再出现

原有 `python3 -m unittest discover -s crates/live_recorder/tests -p 'test_streamlink_worker.py' -v` 仍为 **5 failures / 2 errors**，与迁移分支基线一致。包括 SOOP 认证别名契约、旧 inline worker 命令长度假设和媒体夹具行为。本轮没有修改此 Python 源码或测试，不将其隐藏或标为通过。继续提交到未合并的 `feat/gpui-fast-kit-i18n` / PR #40，保留其 ready 状态；三平台 CI 结果以对应提交的 Actions 为准。

### 云 Linux 原生交互验收

使用隔离 profile、离线 yt-dlp 夹具和 localhost 停滞 HTTP 服务：

- Home 无效 URL 提示、Enter 解析、默认格式、慢解析 Cancel → 新 URL → 旧结果到达后新输入/结果不变
- enqueue 确认后进入 Tasks；离线夹具失败显示真实原因；Delete → Go back 保留记录
- 完整进程重启后失败记录保留；Retry 实际启动进程、计数增加并收到新的终态，没有卡死 Queued
- Capture Enter 启动、停滞 HTTP 期间 Stop、再次 Start/Stop，控件及时恢复可用且输入保留
- 录制 Add Room 打开/取消，非法输入不会关闭编辑窗口

这些交互没有使用真实平台凭据，未代替长时直播/真实下载验收。自动回归覆盖本地 HTTP 完成、取消、续传与重启后重试成功。

## 明确保留的风险 / 后续工作

1. 配置与任务历史中的凭据仍明文保存。Unix 私有权限只减少其他本机用户读取风险；Windows 依赖目录 ACL。未来应迁移系统 keychain/凭据引用，并提供损坏配置恢复 UI
2. 本轮关闭初始平台错选 Cookie；并未完整实现 yt-dlp/ffmpeg/CDN/重定向每一跳的 origin-scoped Cookie jar。源平台凭据随媒体子请求传播的策略仍须单独设计
3. 外部工具 stdout/stderr、其他历史日志路径、文件名及 URL path 仍可能包含敏感信息。不可公开粘贴未审查的完整日志；未实现全量中央日志清洗或保证系统进程参数不含凭据
4. run identity 降低过期回调问题，但任务 map、持久化和事件仍用不同锁；已进入发布阶段的回调与删除/控制动作仍需统一事务化。持久化同 URL 去重仍会合并有意重复下载
5. 续传没有保存 ETag/Last-Modified 并发 If-Range；远端同 URL 内容变更仍可能破坏文件一致性。模板是词法检查，不能抵御符号链接/同名并发写入
6. yt-dlp SHA-256 来自同一 HTTPS 发布者，不等于独立签名信任链。Deno 缓冲后限额/替换回滚、FFmpeg 平台安装策略、macOS/Windows 实际安装替换需要专项测试
7. Home/Channel Cancel 使界面请求失效，已启动的解析线程可能继续到结束；Stop adding 在当前 enqueue 结束后停止。kill-on-drop 只保证自有直接 child，不能声称跨平台任意后代树都被取消
8. 没有验证真实登录/付费内容、线上直播长时重连、所有平台和网络代理协议，也未运行完整依赖漏洞数据库审计。跨平台编译通过不等于跨平台原生交互通过

更新校验依据：[yt-dlp 官方发布文件](https://github.com/yt-dlp/yt-dlp#release-files)、[官方 nightly releases](https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest)。


## 2026-09-30 跟进：Deno 安装与封面清晰度

- Windows 安装失败已用官方 v2.9.7 发布文件确认：Deno Windows `.sha256sum` 是 PowerShell `Algorithm / Hash / Path` 格式，原实现只接受首列为摘要的 GNU 格式。现仅接受官方这两种格式，绑定 ZIP 文件名和唯一 SHA-256，支持可选 UTF-8 BOM；错误算法、重复字段、错文件名、HTML 响应仍拒绝。版本、校验文件、压缩包按流限制大小，解压、SHA 和实际运行版本检查失败会保留现有运行时。
- 官方源证据：[Windows 生成步骤](https://github.com/denoland/deno/blob/v2.9.7/.github/workflows/ci.ts#L901-L903)。13 项针对性回归通过，另以官方 Linux v2.9.7 完成真实下载、校验、解压、执行和隔离目录安装；Windows 实际执行交由新增 CI 检查，不能把 Linux 结果当作 Windows 验收。
- Douyin 原先优先选 `avatar_thumb`，现优先原始封面、直播封面及较大的头像，保留签名 URL；网页多个 OpenGraph 图片按已声明尺寸优先选更大来源。缓存每次启动重新确认来源、过期后重新获取，刷新成功后清除 GPUI 已解码的旧图，原图字节保留。封面显示采用等比缩小，避免把小头像放大铺满卡片。
- 封面测试：5 项 Rust 来源/缓存回归与 4 项 Python 来源排序、字节保留和失败回退测试通过。站点仅提供低分辨率源时无法凭空恢复细节；本轮不做签名 URL 尺寸参数猜改或人工锐化。
- 既有 Douyin 处理器包含公开源码中长期硬编码的 Cookie/访客会话常量；本轮没有加入用户凭证。这些值可能失效并触发站点风控，仍是维护风险，后续应替换为规范的访客会话获取策略。


## 2026-09-30 跟进：现代化布局

本轮调整了页面结构，而不只是控件状态：品牌与分组侧栏、当前页面导航、首页链接工作区与步骤引导、直播间响应式卡片、下载概览与分类标签、工具双列卡片和运行环境区、设置分类页签。交互控件、GroupBox、Tag、TabBar、Empty、主题颜色及图标均来自 gpui-kit；没有新增自有设计系统。

- 云端原生 GUI 已查看首页、工具、带本地测试封面的直播间、失败任务及设置两类页签，保留异步状态和危险操作确认；独立审查发现的“全局自动录制关闭时不能编辑单房间偏好”已修复。
- 原生验收发现两项额外问题并修复：完整 Kit 图标目录没有注册导致部分图标为空；启动时没有应用已保存主题。已通过真实重启确认中文界面和深色主题恢复。
- 工作区测试 174 项通过，12 项忽略（其中一项是需联网的真实 Deno 安装测试）；另有 4 项 Python 封面回归通过。Deno/封面首个提交 58f682f 已在 Windows/macOS/Linux CI 全通过，包括 Windows 从官方源真实下载安装并执行 Deno。
- 原有 Python adapter 的 5 failures、2 errors 仍独立披露；本轮封面测试不替代该基线。原始用户截图已通过会话本地文件恢复并逐一检查，确认模糊区域为 Douyin 小头像裁剪、Deno 错误为校验文件解析失败。原生 GUI 验收环境为云 Linux，Windows/macOS 图形显示不在此声明范围。

- 后续 Linux CI 发现刚写入的 Deno 可执行文件偶发 `ETXTBSY`。版本探测仅对 Linux 的该错误最多重试 5 次、每次 50ms，仍受整体 10 秒超时约束；其他错误立即返回。新增两个确定性测试覆盖短暂可写句柄释放后的恢复和持续占用时的有界失败，15 项 Deno 回归连续运行 10 轮全部通过。

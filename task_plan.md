# 2026-09-24：退出应用后 Python 进程残留

目标：定位由 MageKit 启动的 Python 子进程在正常退出后的残留路径，修复进程所有权与退出清理，并做针对性验证。

- [complete] 梳理 GUI 退出流程、Streamlink 探测/录制 worker 及其子进程
- [complete] 修复正常退出和异步任务中断时的子进程清理
- [complete] 运行针对性测试、格式和编译检查，记录限制

## 本轮错误

- 首次并行工具脚本有括号语法错误；已修正后正常执行。
- 初次全文检索把 UI `.child` 误纳入结果，已改为定向检索。
- 规划文件第一次补丁误判 `progress.md` 标题，整体补丁未应用；已按实际标题重试。
- 对整个 Cargo registry 做符号搜索耗时过长，已终止并限定到 `windows-sys 0.61.2` 模块；Tokio 源码通配路径在 PowerShell 中无效，改用子进程 PID/Win32 API 路线。

# Streamlink-only 录制切换

## 2026-09-23：SOOP 原画档与连接时间（完成）

- [complete] 对照 `문채원♡` 的 SOOP 直播档位及成品分辨率，查明 `Original` 可取得 1080p，而历史成品是 720p
- [complete] 原画选项禁止首包慢时静默降到次档，并在录制日志显示实际选档
- [complete] 复用房态频道元数据，后台预热手动录制的短期播放授权；预热过期/失效时回退重新授权
- [complete] 当前房间有界实录验证 1080p 原画；预热后的实录首批媒体约 9 秒到达
- [complete] Python 语法、格式、Cargo 检查与 debug 构建

## 2026-09-23：SOOP 韩文路径与假 REC（代码完成）

- [complete] 对照用户实际目录、运行进程和 worker，定位 GBK 误解 UTF-8 路径
- [complete] 显式 UTF-8 解码、真实输出文件字节校验，并避免 Python 终态线程崩溃
- [complete] 录制页进度/终态绑定任务 ID，防止旧任务回调污染新录制
- [complete] 检查恢复文件哈希；用户已自行删除文件，停止后续恢复
- [complete] 格式、语法、编译和 debug 构建
- [pending] SOOP 房间再次开播后，以真实直播复核含韩文路径的新录制

## 2026-09-23：SOOP 显示连接但输出零字节（完成）

- [complete] 用当前房间有界诊断 pull 与 record_direct_ts 两段数据流，定位首字节丢失位置
- [complete] 补齐后台/手动停止日志，确保零字节与 worker 错误能在日志和界面中区分
- [complete] 修复录制阶段零字节等待/传输问题，并完成有界实际录制验证
- [complete] 格式检查、编译与 debug 构建

## 2026-09-23：SOOP 启动耗时与工具页布局（完成）

- [complete] 将 SOOP 录制启动从“全清晰度预检”改为单档流授权解析，并保留非 GUI/模板路径的兼容预检
- [complete] 将 Deno、Streamlink 环境卡片移到 yt-dlp/FFmpeg 之后、“关于工具”之前，并压缩成并排卡片
- [complete] 格式化、静态检查、debug 构建并记录结果（未运行测试套件）

## 2026-09-23：抖音封面、SOOP 检查速度与系统代理

- [complete] 确认抖音离线响应通常没有直播封面字段，保存的图片 URL 是远程链接；补主播头像回退并缓存抖音图片到本地
- [complete] 确认 SOOP 状态轮询曾解析全部 HLS 清晰度并额外抓取直播页封面；改为频道元数据状态查询，跳过轮询期的封面网页请求并缩短请求超时
- [complete] 将设置页的代理模式传入 Streamlink；“系统代理”读取 Windows 当前用户代理配置，避免 `http-trust-env=false` 绕过本机代理
- [complete] 格式、Cargo 检查、Python 语法、diff 检查和 debug 构建通过；未运行测试套件

## 2026-09-23：Windows Streamlink worker 命令过长

- [complete] 定位多平台统一出现的 `os error 206`：将约 36K 字符的 Python worker 源码直接传给 `python -c`，超过 Windows 进程命令行长度上限
- [complete] 改为按源码摘要将 worker 原子写入受管 Python 环境，并以脚本路径启动探测/录制进程
- [complete] 录制 worker 的 pull 子进程也切换到脚本路径，避免只有父进程修复后子进程仍失败
- [complete] Python 语法、Cargo 检查、debug 构建和 diff 检查通过

## 2026-09-23：封面、录制页卡顿与应用图标

- [complete] 补齐 Streamlink 平台封面元数据，缓存图片到本地；保留抖音原生封面路径
- [complete] 限制房间探测并发、阻止重叠轮询并将轮询写盘移出 UI 回调
- [complete] 生成 MageKit 图标并接入 Windows、macOS、Linux 打包资源
- [complete] 完成格式、diff、Python 语法、Cargo 检查与 Windows EXE 图标提取核验

## 目标

增加一个可配置的“仅使用 Streamlink”模式。开启时隐藏/禁用旧的原生录制入口，并让录制流程只走 Streamlink，方便用户测试；默认保留现有兼容行为，避免影响已有配置。

## 阶段

- [complete] 梳理配置、设置页、录制页和录制调度链路
- [complete] 增加配置字段并接入持久化/设置 UI
- [complete] 在入口和调度层应用 Streamlink-only 行为
- [complete] 编译、测试、检查 warning 并记录结果

## 本轮修复（SOOP 录制与首次刷新）

- [complete] 复现并定位 Streamlink worker 退出和首次状态刷新问题
- [complete] 修复 worker 错误传播、进度状态和启动检查时序
- [complete] 针对 SOOP 做 URL/请求参数兼容处理并验证
- [complete] 编译、测试并检查 warning

## 本轮修复（启动闪退与失效封面）

- [complete] 定位图片 403/404 与 Taffy 布局 panic 的触发路径
- [complete] 保留启动时的历史封面缓存，并为远程图片增加失败占位
- [complete] 刷新成功后用新封面替换旧缓存，完成编译、测试和运行回归

## 本轮修正（保留历史封面过渡）

- [complete] 恢复启动时的历史封面显示
- [complete] 仅在拿到新封面时替换缓存，失败时由 fallback 回到占位图
- [complete] 完成回归并确认无 panic

## 决策

- 优先做可切换开关，而不是删除原生录制代码；这样本次测试可以只启用 Streamlink，后续仍可恢复。
- 用户当前明确要测试 Streamlink，因此开关默认开启；旧配置中缺少该字段时也默认开启，确保本次启动不会意外回退到原生录制。

## 错误记录

| 错误 | 尝试 | 处理 |
| --- | --- | --- |
| 录制页链式 GPUI 表达式语法错误 | 首次插入引擎标签后运行 `cargo fmt` | 将标签放回同一个链式 `div()` 表达式后通过格式化 |
| `IconName::Video` 不存在 | 编译检查发现枚举没有该成员 | 改用已存在的 `IconName::Globe` |
| SOOP 录制失败原因不可见 | worker 将 pull/ffmpeg stderr 丢弃，UI 将进度通道关闭当作完成 | 已保留安全错误摘要，并让 UI 等待最终状态后标记失败 |

## 2026-09-23：SOOP Cookie 与虎牙探测响应

- [complete] 核对 Streamlink 8.6.1 SOOP 插件的认证/API 域名与 Cookie 设置平台键
- [complete] 让“SOOP 韩国”Cookie 在 SOOP 官方 `.com` API 域可用，并保留 Global Cookie 优先级
- [complete] 区分 Cookie 已送达但登录校验失败的提示，并在设置页说明域名行为
- [complete] 修复探测协议按 JSON Lines 解析，并为无效响应补充不含 Cookie/URL 的安全诊断
- [complete] 完成格式与编译检查，不运行测试套件

## 2026-09-23：SOOP 名称/录制与虎牙封面

- [complete] 修正添加失败后的主播名占位，使周期检查和手动刷新成功后都能回填
- [complete] 恢复 SOOP 录制复用预检 HLS 地址，并为 TS 输出恢复直写路径
- [complete] 增加虎牙官方房间 API 封面回退，缓存直播截图并校验可信图片域
- [complete] 连接阶段显示“连接中”，有输出文件字节后才显示 REC 计时
- [complete] 格式、Python 语法、Cargo 检查和 debug 可执行文件构建通过；未运行测试套件

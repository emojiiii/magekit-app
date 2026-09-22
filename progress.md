# 进度记录

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

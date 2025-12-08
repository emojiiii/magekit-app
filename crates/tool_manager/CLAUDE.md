# CLAUDE - tool_manager
Breadcrumb: Home / tool_manager

## 角色
管理外部工具（yt-dlp/ffmpeg）、调度下载任务、持久化与队列控制，向 GUI 暴露下载/信息查询接口。

## 关键文件
- `src/task_manager.rs`：核心 `ToolManager`（`ToolStorage`、`VideoDownloader`、`ConfigManager`、任务 map、`TaskQueue`、`TaskPersistence`、事件 tx）。提供 ensure/check updates、get_video_info/get_channel_videos、start_download、pause/resume/cancel、队列与重试、批量操作、速度限制、持久化恢复。
- `src/downloader.rs`：封装 yt-dlp/ffmpeg 调用（Tokio 进程），支持 Cookie 文件、进度解析、频道列表（flat-playlist，含 Bilibili API 兼容）、输出路径探测。
- `src/config.rs`：`ToolManagerConfig`（tools + download_defaults + advanced），配置文件 `tool_manager.toml` 存于应用配置目录；支持 merge from AppConfig、校验、更新工具路径/通道/并发数。
- 其他模块（未细扫）：`storage.rs`（工具二进制存储）、`updater.rs`（工具下载/更新）、`task_queue.rs`（并发/优先级）、`task_persistence.rs`（磁盘持久化）、`history.rs`（历史）。

## 数据流与生命周期
- 初始化：`ToolManager::new_sync` → 初始化存储/配置/任务队列/持久化，推断 ffmpeg 路径（应用内→系统 PATH）。
- 任务创建：`start_download` 生成 TaskId/状态，查询视频信息补全输出路径，发送 TaskUpdate::Created，spawn 下载任务；进度通过 mpsc 反馈并更新 TaskStatus + 持久化。
- 队列：`TaskQueue::new(max_concurrent)` 控制并发；支持 enqueue、优先级、统计、暂停/恢复/清空。
- 重试与速度：`retry_task/auto_retry_if_possible`，`set_speed_limit/get_speed_limit`。
- 持久化：`TaskPersistence` 保存/恢复任务；`clear_completed/save_tasks` 辅助清理与持久化。

## 依赖
- `magekit-shared` 类型/配置/路径工具；`tokio`、`which`、`reqwest`（stream）、`tempfile`；内部使用 mpsc/RwLock/Mutex。

## 注意事项
- `start_download` 内部当前复用 `DownloadOptions::default` 给重试队列（TODO：补原始选项持久化）。
- 事件通道 `update_tx` 目前未对外转发（subscribe 返回空 rx）—GUI 需要自行 poll/更新。
- 校验：`ConfigManager::validate` 检查并发/超时/重试/格式，不足需在 UI 显示。

## 推荐下一步
- 审阅 `storage.rs` 和 `updater.rs`，确认工具下载/升级与平台路径。
- 完善 TaskQueue 绑定 GUI：传递更新事件而不是空订阅；补测试覆盖下载/队列/持久化路径。


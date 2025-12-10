# magekit-tool-manager

外部工具与下载任务的统一管理层，负责 yt-dlp / ffmpeg 的安装更新、视频信息获取、任务调度与持久化。

## 核心职责

- 工具管理：下载、更新、存储 yt-dlp / ffmpeg，跨平台路径抽象
- 信息获取：`get_video_info` / `get_channel_videos`
- 下载任务：`start_download` / `start_download_with_info`，进度回调与事件广播
- 队列与并发：`enqueue_download`、优先级控制、队列暂停/恢复/清空
- 状态管理：任务暂停/恢复/取消、速度限制、自动重试、持久化恢复
- 配置与历史：`ConfigManager`、`HistoryManager`

## 主要 API

- 构造
  - `ToolManager::new()` / `new_sync()`
  - `subscribe()` 获取 `broadcast::Receiver<ToolManagerEvent>`
- 工具
  - `ensure_tools(channel)`、`check_for_updates(channel)`、`updater()`
  - `storage()` 返回 `ToolStorage`
- 下载
  - `get_video_info(url, cookies)`、`get_channel_videos(url, cookies)`
  - `start_download(url, options, cookies)` / `start_download_with_info(...)`
  - 任务控制：`pause_download`、`resume_download`、`cancel_download`
  - 状态：`get_task_status`、`get_all_tasks`
  - 批量：`pause_all`、`resume_all`、`cancel_all`
- 队列与重试
  - `enqueue_download`、`get_queue_stats`、`set_task_priority`
  - `retry_task`、`auto_retry_if_possible`
- 速度/持久化
  - `set_speed_limit`、`get_speed_limit`
  - `restore_tasks`、`clear_completed_tasks`、`save_tasks`

事件通过 `ToolManagerEvent` 广播，包括 `TaskUpdate`、`ToolUpdate` 和队列事件。

## 简单示例

```rust
use magekit_tool_manager::ToolManager;
use magekit_shared::{DownloadOptions, UpdateChannel};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let tm = ToolManager::new().await?;
    tm.ensure_tools(UpdateChannel::Stable).await?;

    let task_id = tm
        .start_download("https://youtu.be/xxx", DownloadOptions::default(), None)
        .await?;
    println!("任务: {}", task_id);

    let mut rx = tm.subscribe();
    while let Ok(event) = rx.recv().await {
        println!("事件: {:?}", event);
    }
    Ok(())
}
```

## 优点

- 事件驱动 + 持久化的任务管理，可恢复、可批量控制。
- 队列与优先级内建，避免 UI / 上层重复实现调度。
- 工具存储路径抽象，兼容多平台与便携目录。

## 局限 / 风险

- 模块体积较大，`ToolManager` 职责宽泛（工具、任务、配置、历史）耦合度高。
- 任务状态与持久化读写交叉，缺少事务性保障，异常时可能状态不一致。
- yt-dlp/ffmpeg 版本策略简单，缺少版本钉住与校验。

## 改进建议

- 拆分“工具生命周期管理”和“下载任务调度”为两个子模块/trait，明确边界。
- 为任务持久化增加校验与迁移策略（版本号、校验和），避免脏数据。
- 引入可配置的下载并发/限速策略，并暴露清晰的错误分类与重试策略。
- 增加基准/健康检查：工具可用性、磁盘空间、网络速率，提前失败而非中途报错。


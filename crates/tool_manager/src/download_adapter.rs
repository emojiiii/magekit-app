//! Download 库适配器
//!
//! 将 download 库的回调接口适配到 tool_manager 的任务系统

use download::{DownloadCallback, DownloadError as DlError, DownloadOutcome, DownloadProgress as DlProgress, LogLine};
use magekit_shared::{TaskId, TaskUpdate};
use tokio::sync::mpsc;

/// 任务进度回调适配器
pub struct TaskProgressCallback {
    task_id: TaskId,
    update_tx: mpsc::Sender<TaskUpdate>,
}

impl TaskProgressCallback {
    pub fn new(task_id: TaskId, update_tx: mpsc::Sender<TaskUpdate>) -> Self {
        Self {
            task_id,
            update_tx,
        }
    }

    /// 发送任务更新（内部辅助方法）
    fn send_update(&self, update: TaskUpdate) {
        let tx = self.update_tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(update).await;
        });
    }
}

impl DownloadCallback for TaskProgressCallback {
    fn on_progress(&self, progress: DlProgress) {
        // 计算进度百分比
        let percent = if let Some(total) = progress.total_bytes {
            if total > 0 {
                (progress.bytes_downloaded as f32) / (total as f32)
            } else {
                0.0
            }
        } else {
            0.0
        };

        // 使用 TaskUpdate::Progress 枚举变体
        let update = TaskUpdate::Progress(
            self.task_id,
            percent,
            progress.bytes_downloaded,
            progress.total_bytes,
            progress.speed_bps,
            progress.eta,
        );

        self.send_update(update);

        // 打印日志
        let stage_str = match progress.stage {
            download::DownloadStage::Preparing => "准备中",
            download::DownloadStage::Downloading => "下载中",
            download::DownloadStage::Merging => "合并中",
            download::DownloadStage::Completed => "已完成",
        };

        tracing::info!(
            "📥 任务 {} {}: {:.1}% ({} / {})",
            self.task_id,
            stage_str,
            percent * 100.0,
            format_bytes(progress.bytes_downloaded),
            progress.total_bytes.map_or("未知".to_string(), |t| format_bytes(t))
        );
    }

    fn on_complete(&self, outcome: DownloadOutcome) {
        tracing::info!("✅ 任务 {} 下载完成: {:?}", self.task_id, outcome.output_path);

        // 使用 TaskUpdate::Completed 枚举变体
        let update = TaskUpdate::Completed(self.task_id, outcome.output_path);

        self.send_update(update);
    }

    fn on_error(&self, error: DlError) {
        tracing::error!("❌ 任务 {} 下载失败: {}", self.task_id, error);

        // 使用 TaskUpdate::Failed 枚举变体
        let update = TaskUpdate::Failed(self.task_id, format!("下载失败: {}", error));

        self.send_update(update);
    }

    fn on_log(&self, log: LogLine) {
        // 打印日志
        let prefix = match log.source {
            download::LogSource::Stdout => "stdout",
            download::LogSource::Stderr => "stderr",
            download::LogSource::System => "system",
        };

        tracing::debug!("[{}] {}", prefix, log.line);
    }
}

/// 格式化字节数为人类可读的格式
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

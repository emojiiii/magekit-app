//! Download 库适配器
//!
//! 将 download 库的回调接口适配到 tool_manager 的任务系统

use download::{
    DownloadCallback, DownloadError as DlError, DownloadOutcome, DownloadProgress as DlProgress,
    LogLine,
};
use magekit_shared::{TaskId, TaskUpdate};
use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

/// 任务进度回调适配器
pub struct TaskProgressCallback {
    task_id: TaskId,
    update_tx: mpsc::Sender<TaskUpdate>,
    last_emit_ms: AtomicU64,
    last_percent_x10000: AtomicU32,
    last_stage: AtomicU8,
}

impl TaskProgressCallback {
    pub fn new(task_id: TaskId, update_tx: mpsc::Sender<TaskUpdate>) -> Self {
        Self {
            task_id,
            update_tx,
            last_emit_ms: AtomicU64::new(0),
            last_percent_x10000: AtomicU32::new(0),
            last_stage: AtomicU8::new(u8::MAX),
        }
    }

    /// 发送任务更新（最佳努力，不阻塞调用方）
    ///
    /// 注意：`DownloadCallback` 为同步 trait；如果每次都 `tokio::spawn + send().await`，
    /// 在高频进度更新下会引入大量 task 调度开销，进而导致“任务状态更新不及时”。
    fn send_update_best_effort(&self, update: TaskUpdate) {
        let _ = self.update_tx.try_send(update);
    }

    /// 发送任务更新（尽力投递：用于 Completed/Failed 等关键事件）
    fn send_update_reliable(&self, update: TaskUpdate) {
        match self.update_tx.try_send(update) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(update)) => {
                let tx = self.update_tx.clone();
                tokio::spawn(async move {
                    let _ = tx.send(update).await;
                });
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {}
        }
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    fn stage_id(stage: download::DownloadStage) -> u8 {
        match stage {
            download::DownloadStage::Preparing => 0,
            download::DownloadStage::Downloading => 1,
            download::DownloadStage::Merging => 2,
            download::DownloadStage::Completed => 3,
        }
    }

    /// 判断是否需要发送 Progress 更新（节流：默认最多 5Hz，或阶段切换/进度跃迁时立即发送）
    fn should_emit_progress(&self, progress: &DlProgress, percent_x10000: u32) -> bool {
        const MIN_INTERVAL_MS: u64 = 200;
        const MIN_DELTA_X10000: u32 = 10; // 0.1%

        let now_ms = Self::now_ms();
        let last_ms = self.last_emit_ms.load(Ordering::Relaxed);
        let last_percent = self.last_percent_x10000.load(Ordering::Relaxed);

        let stage_id = Self::stage_id(progress.stage.clone());
        let last_stage = self.last_stage.load(Ordering::Relaxed);

        let stage_changed = stage_id != last_stage;
        let time_ok = now_ms.saturating_sub(last_ms) >= MIN_INTERVAL_MS;
        let percent_ok = percent_x10000.saturating_sub(last_percent) >= MIN_DELTA_X10000;

        if stage_changed || time_ok || percent_ok {
            self.last_emit_ms.store(now_ms, Ordering::Relaxed);
            self.last_percent_x10000
                .store(percent_x10000, Ordering::Relaxed);
            self.last_stage.store(stage_id, Ordering::Relaxed);
            true
        } else {
            false
        }
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

        let percent_x10000 = (percent.clamp(0.0, 1.0) * 10000.0).round() as u32;
        if !self.should_emit_progress(&progress, percent_x10000) {
            return;
        }

        // 使用 TaskUpdate::Progress 枚举变体
        self.send_update_best_effort(TaskUpdate::Progress(
            self.task_id,
            percent,
            progress.bytes_downloaded,
            progress.total_bytes,
            progress.speed_bps,
            progress.eta,
        ));

        // 打印日志
        let stage_str = match progress.stage {
            download::DownloadStage::Preparing => "准备中",
            download::DownloadStage::Downloading => "下载中",
            download::DownloadStage::Merging => "合并中",
            download::DownloadStage::Completed => "已完成",
        };

        // 进度日志非常频繁：保持在 debug，避免影响任务更新时效
        tracing::debug!(
            "📥 任务 {} {}: {:.1}% ({} / {})",
            self.task_id,
            stage_str,
            percent * 100.0,
            format_bytes(progress.bytes_downloaded),
            progress
                .total_bytes
                .map_or("未知".to_string(), |t| format_bytes(t))
        );
    }

    fn on_complete(&self, outcome: DownloadOutcome) {
        tracing::info!(
            "✅ 任务 {} 下载完成: {:?}",
            self.task_id,
            outcome.output_path
        );

        // 使用 TaskUpdate::Completed 枚举变体
        let update = TaskUpdate::Completed(self.task_id, outcome.output_path);

        self.send_update_reliable(update);
    }

    fn on_error(&self, error: DlError) {
        tracing::error!("❌ 任务 {} 下载失败: {}", self.task_id, error);

        // 使用 TaskUpdate::Failed 枚举变体
        let update = TaskUpdate::Failed(self.task_id, format!("下载失败: {}", error));

        self.send_update_reliable(update);
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

//! 任务队列模块
//!
//! 实现任务队列和并发控制，支持任务优先级管理。

use magekit_shared::{DownloadOptions, TaskId};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore, mpsc};

/// 任务优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Urgent = 3,
}

impl Default for TaskPriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// 带优先级的任务项
#[derive(Debug, Clone)]
pub struct QueuedTask {
    pub task_id: TaskId,
    pub url: String,
    pub options: DownloadOptions,
    pub priority: TaskPriority,
    pub created_at: std::time::Instant,
}

impl PartialEq for QueuedTask {
    fn eq(&self, other: &Self) -> bool {
        self.task_id == other.task_id
    }
}

impl Eq for QueuedTask {}

impl PartialOrd for QueuedTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueuedTask {
    fn cmp(&self, other: &Self) -> Ordering {
        // 优先级高的排前面，同优先级先进先出
        match (self.priority as u8).cmp(&(other.priority as u8)) {
            Ordering::Equal => other.created_at.cmp(&self.created_at), // 早创建的优先
            other => other,
        }
    }
}

/// 任务队列事件
#[derive(Debug, Clone)]
pub enum QueueEvent {
    TaskAdded(TaskId),
    TaskStarted(TaskId),
    TaskCompleted(TaskId),
    TaskFailed(TaskId, String),
    TaskCancelled(TaskId),
    QueueEmpty,
}

/// 任务队列
pub struct TaskQueue {
    /// 等待中的任务（优先级队列）
    pending: Arc<RwLock<BinaryHeap<QueuedTask>>>,
    /// 正在执行的任务
    running: Arc<RwLock<HashMap<TaskId, QueuedTask>>>,
    /// 并发控制信号量
    semaphore: Arc<Semaphore>,
    /// 最大并发数
    max_concurrent: usize,
    /// 事件发送器
    event_tx: mpsc::Sender<QueueEvent>,
    /// 是否已暂停队列
    paused: Arc<RwLock<bool>>,
}

impl TaskQueue {
    /// 创建新的任务队列
    pub fn new(max_concurrent: usize) -> (Self, mpsc::Receiver<QueueEvent>) {
        let (event_tx, event_rx) = mpsc::channel(1000);

        let queue = Self {
            pending: Arc::new(RwLock::new(BinaryHeap::new())),
            running: Arc::new(RwLock::new(HashMap::new())),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
            event_tx,
            paused: Arc::new(RwLock::new(false)),
        };

        (queue, event_rx)
    }

    /// 添加任务到队列
    pub async fn enqueue(&self, task: QueuedTask) -> TaskId {
        let task_id = task.task_id;

        {
            let mut pending = self.pending.write().await;
            pending.push(task);
        }

        let _ = self.event_tx.send(QueueEvent::TaskAdded(task_id)).await;

        tracing::debug!("Task {} added to queue", task_id);
        task_id
    }

    /// 从队列中获取下一个任务
    pub async fn dequeue(&self) -> Option<QueuedTask> {
        // 检查队列是否暂停
        if *self.paused.read().await {
            return None;
        }

        // 尝试获取信号量
        let permit = self.semaphore.clone().try_acquire_owned().ok()?;

        let task = {
            let mut pending = self.pending.write().await;
            pending.pop()
        };

        if let Some(task) = task {
            // 添加到运行中列表
            {
                let mut running = self.running.write().await;
                running.insert(task.task_id, task.clone());
            }

            let _ = self
                .event_tx
                .send(QueueEvent::TaskStarted(task.task_id))
                .await;

            // 保持permit，任务完成时释放
            std::mem::forget(permit);

            Some(task)
        } else {
            // 没有任务，释放permit
            drop(permit);
            None
        }
    }

    /// 标记任务完成
    pub async fn complete(&self, task_id: TaskId) {
        {
            let mut running = self.running.write().await;
            running.remove(&task_id);
        }

        // 释放一个permit
        self.semaphore.add_permits(1);

        let _ = self.event_tx.send(QueueEvent::TaskCompleted(task_id)).await;

        // 检查队列是否为空
        if self.is_empty().await {
            let _ = self.event_tx.send(QueueEvent::QueueEmpty).await;
        }
    }

    /// 标记任务失败
    pub async fn fail(&self, task_id: TaskId, error: String) {
        {
            let mut running = self.running.write().await;
            running.remove(&task_id);
        }

        // 释放一个permit
        self.semaphore.add_permits(1);

        let _ = self
            .event_tx
            .send(QueueEvent::TaskFailed(task_id, error))
            .await;
    }

    /// 取消任务
    pub async fn cancel(&self, task_id: TaskId) -> bool {
        // 先检查是否在等待队列中
        {
            let mut pending = self.pending.write().await;
            let tasks: Vec<_> = std::mem::take(&mut *pending).into_vec();
            let mut found = false;

            for task in tasks {
                if task.task_id == task_id {
                    found = true;
                } else {
                    pending.push(task);
                }
            }

            if found {
                let _ = self.event_tx.send(QueueEvent::TaskCancelled(task_id)).await;
                return true;
            }
        }

        // 检查是否在运行中
        {
            let running = self.running.read().await;
            if running.contains_key(&task_id) {
                // 运行中的任务需要通过其他方式取消
                return true;
            }
        }

        false
    }

    /// 暂停队列
    pub async fn pause(&self) {
        *self.paused.write().await = true;
        tracing::info!("Task queue paused");
    }

    /// 恢复队列
    pub async fn resume(&self) {
        *self.paused.write().await = false;
        tracing::info!("Task queue resumed");
    }

    /// 检查队列是否暂停
    pub async fn is_paused(&self) -> bool {
        *self.paused.read().await
    }

    /// 获取等待中的任务数量
    pub async fn pending_count(&self) -> usize {
        self.pending.read().await.len()
    }

    /// 获取运行中的任务数量
    pub async fn running_count(&self) -> usize {
        self.running.read().await.len()
    }

    /// 检查队列是否为空
    pub async fn is_empty(&self) -> bool {
        self.pending.read().await.is_empty() && self.running.read().await.is_empty()
    }

    /// 获取最大并发数
    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }

    /// 调整最大并发数
    pub fn set_max_concurrent(&mut self, max_concurrent: usize) {
        let diff = max_concurrent as isize - self.max_concurrent as isize;

        if diff > 0 {
            self.semaphore.add_permits(diff as usize);
        }
        // 注意：减少并发数时，已获取的permits会在任务完成后自然释放

        self.max_concurrent = max_concurrent;
    }

    /// 清空队列
    pub async fn clear(&self) {
        let mut pending = self.pending.write().await;
        while let Some(task) = pending.pop() {
            let _ = self
                .event_tx
                .send(QueueEvent::TaskCancelled(task.task_id))
                .await;
        }
    }

    /// 获取队列统计信息
    pub async fn stats(&self) -> QueueStats {
        QueueStats {
            pending: self.pending_count().await,
            running: self.running_count().await,
            max_concurrent: self.max_concurrent,
            paused: self.is_paused().await,
        }
    }

    /// 修改任务优先级
    pub async fn set_priority(&self, task_id: TaskId, priority: TaskPriority) -> bool {
        let mut pending = self.pending.write().await;
        let tasks: Vec<_> = std::mem::take(&mut *pending).into_vec();
        let mut found = false;

        for mut task in tasks {
            if task.task_id == task_id {
                task.priority = priority;
                found = true;
            }
            pending.push(task);
        }

        found
    }
}

/// 队列统计信息
#[derive(Debug, Clone)]
pub struct QueueStats {
    pub pending: usize,
    pub running: usize,
    pub max_concurrent: usize,
    pub paused: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn create_test_task(priority: TaskPriority) -> QueuedTask {
        QueuedTask {
            task_id: Uuid::new_v4(),
            url: "https://example.com/video".to_string(),
            options: DownloadOptions::default(),
            priority,
            created_at: std::time::Instant::now(),
        }
    }

    #[tokio::test]
    async fn test_queue_priority() {
        let (queue, _rx) = TaskQueue::new(1);

        let low = create_test_task(TaskPriority::Low);
        let high = create_test_task(TaskPriority::High);
        let normal = create_test_task(TaskPriority::Normal);

        queue.enqueue(low.clone()).await;
        queue.enqueue(high.clone()).await;
        queue.enqueue(normal.clone()).await;

        // 高优先级应该先出队
        let first = queue.dequeue().await.unwrap();
        assert_eq!(first.task_id, high.task_id);
    }
}

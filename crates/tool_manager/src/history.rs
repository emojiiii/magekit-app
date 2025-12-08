//! 下载历史记录模块
//!
//! 管理下载历史记录的持久化和查询

use magekit_shared::types::{TaskId, TaskState, TaskStatus};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::SystemTime;

/// 单条历史记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// 任务 ID
    pub task_id: TaskId,
    /// 视频标题
    pub title: String,
    /// 原始 URL
    pub url: String,
    /// 输出文件路径
    pub output_path: Option<PathBuf>,
    /// 文件大小（字节）
    pub file_size: Option<u64>,
    /// 完成时间
    pub completed_at: SystemTime,
    /// 最终状态
    pub final_state: HistoryState,
}

/// 历史记录状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HistoryState {
    /// 下载完成
    Completed,
    /// 下载失败
    Failed(String),
    /// 下载取消
    Cancelled,
}

impl From<TaskState> for HistoryState {
    fn from(state: TaskState) -> Self {
        match state {
            TaskState::Completed => HistoryState::Completed,
            TaskState::Failed(err) => HistoryState::Failed(err),
            TaskState::Cancelled => HistoryState::Cancelled,
            _ => HistoryState::Cancelled, // 其他状态不应该进入历史
        }
    }
}

impl HistoryEntry {
    /// 从任务状态创建历史记录
    pub fn from_task(status: &TaskStatus) -> Self {
        Self {
            task_id: status.id,
            title: status
                .title
                .clone()
                .unwrap_or_else(|| "未知标题".to_string()),
            url: status.url.clone(),
            output_path: status.output_path.clone(),
            file_size: status.total_bytes,
            completed_at: status.completed_at.unwrap_or_else(SystemTime::now),
            final_state: status.state.clone().into(),
        }
    }

    /// 是否成功完成
    pub fn is_successful(&self) -> bool {
        matches!(self.final_state, HistoryState::Completed)
    }
}

/// 历史记录存储
#[derive(Debug, Serialize, Deserialize)]
pub struct HistoryStore {
    /// 历史记录列表（最新的在前面）
    entries: VecDeque<HistoryEntry>,
    /// 最大记录数量
    #[serde(default = "default_max_entries")]
    max_entries: usize,
    /// 版本号
    #[serde(default)]
    version: u32,
}

fn default_max_entries() -> usize {
    1000
}

impl Default for HistoryStore {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries: default_max_entries(),
            version: 1,
        }
    }
}

impl HistoryStore {
    /// 创建新的历史记录存储
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries,
            version: 1,
        }
    }

    /// 添加历史记录
    pub fn add(&mut self, entry: HistoryEntry) {
        // 移除同一任务的旧记录（如果存在）
        self.entries.retain(|e| e.task_id != entry.task_id);

        // 添加到前面
        self.entries.push_front(entry);

        // 限制数量
        while self.entries.len() > self.max_entries {
            self.entries.pop_back();
        }
    }

    /// 从任务状态添加历史记录
    pub fn add_from_task(&mut self, status: &TaskStatus) {
        if status.is_finished() {
            self.add(HistoryEntry::from_task(status));
        }
    }

    /// 获取所有历史记录
    pub fn entries(&self) -> impl Iterator<Item = &HistoryEntry> {
        self.entries.iter()
    }

    /// 获取最近 N 条记录
    pub fn recent(&self, count: usize) -> impl Iterator<Item = &HistoryEntry> {
        self.entries.iter().take(count)
    }

    /// 搜索历史记录
    pub fn search(&self, query: &str) -> Vec<&HistoryEntry> {
        let query_lower = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| {
                e.title.to_lowercase().contains(&query_lower)
                    || e.url.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// 按状态过滤
    pub fn filter_by_state(&self, state: &HistoryState) -> Vec<&HistoryEntry> {
        self.entries
            .iter()
            .filter(|e| &e.final_state == state)
            .collect()
    }

    /// 删除历史记录
    pub fn remove(&mut self, task_id: TaskId) -> bool {
        let len_before = self.entries.len();
        self.entries.retain(|e| e.task_id != task_id);
        self.entries.len() < len_before
    }

    /// 清空所有历史记录
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// 清空失败和取消的记录
    pub fn clear_unsuccessful(&mut self) {
        self.entries.retain(|e| e.is_successful());
    }

    /// 历史记录数量
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 获取统计信息
    pub fn stats(&self) -> HistoryStats {
        let mut stats = HistoryStats::default();
        for entry in &self.entries {
            stats.total += 1;
            match &entry.final_state {
                HistoryState::Completed => {
                    stats.successful += 1;
                    if let Some(size) = entry.file_size {
                        stats.total_bytes += size;
                    }
                }
                HistoryState::Failed(_) => stats.failed += 1,
                HistoryState::Cancelled => stats.cancelled += 1,
            }
        }
        stats
    }
}

/// 历史记录统计
#[derive(Debug, Clone, Default)]
pub struct HistoryStats {
    /// 总记录数
    pub total: usize,
    /// 成功数量
    pub successful: usize,
    /// 失败数量
    pub failed: usize,
    /// 取消数量
    pub cancelled: usize,
    /// 总下载字节数
    pub total_bytes: u64,
}

/// 历史记录管理器
pub struct HistoryManager {
    /// 历史记录存储
    store: HistoryStore,
    /// 存储路径
    path: PathBuf,
    /// 是否有未保存的更改
    dirty: bool,
}

impl HistoryManager {
    /// 创建新的历史记录管理器
    pub fn new(path: PathBuf) -> Self {
        Self {
            store: HistoryStore::default(),
            path,
            dirty: false,
        }
    }

    /// 从文件加载历史记录
    pub async fn load(&mut self) -> Result<(), std::io::Error> {
        if self.path.exists() {
            let content = tokio::fs::read_to_string(&self.path).await?;
            self.store = serde_json::from_str(&content)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        }
        self.dirty = false;
        Ok(())
    }

    /// 保存历史记录到文件
    pub async fn save(&mut self) -> Result<(), std::io::Error> {
        // 确保父目录存在
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let content = serde_json::to_string_pretty(&self.store)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        tokio::fs::write(&self.path, content).await?;
        self.dirty = false;
        Ok(())
    }

    /// 添加历史记录
    pub fn add(&mut self, entry: HistoryEntry) {
        self.store.add(entry);
        self.dirty = true;
    }

    /// 从任务状态添加历史记录
    pub fn add_from_task(&mut self, status: &TaskStatus) {
        self.store.add_from_task(status);
        self.dirty = true;
    }

    /// 获取历史记录存储的引用
    pub fn store(&self) -> &HistoryStore {
        &self.store
    }

    /// 获取历史记录存储的可变引用
    pub fn store_mut(&mut self) -> &mut HistoryStore {
        self.dirty = true;
        &mut self.store
    }

    /// 是否有未保存的更改
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// 自动保存（如果有未保存的更改）
    pub async fn auto_save(&mut self) -> Result<(), std::io::Error> {
        if self.dirty {
            self.save().await?;
        }
        Ok(())
    }
}

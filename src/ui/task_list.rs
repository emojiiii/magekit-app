//! 任务列表组件
//!
//! 显示下载任务的列表视图，支持任务操作（暂停、继续、取消、删除）

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::*;
use magekit_shared::truncate_string;
use magekit_shared::types::{TaskId, TaskState, TaskStatus};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 任务操作回调类型
pub type TaskActionCallback = Arc<dyn Fn(TaskId, TaskAction) + Send + Sync + 'static>;

/// 任务操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskAction {
    /// 暂停任务
    Pause,
    /// 继续任务
    Resume,
    /// 取消任务
    Cancel,
    /// 删除任务
    Delete,
    /// 重试任务
    Retry,
    /// 打开文件
    OpenFile,
    /// 打开文件所在目录
    OpenFolder,
}

/// 任务列表数据
pub struct TaskListData {
    /// 任务状态映射
    tasks: HashMap<TaskId, TaskStatus>,
    /// 任务顺序
    order: Vec<TaskId>,
}

impl TaskListData {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            order: Vec::new(),
        }
    }

    /// 添加任务
    pub fn add_task(&mut self, status: TaskStatus) {
        let id = status.id;
        if !self.tasks.contains_key(&id) {
            self.order.push(id);
        }
        self.tasks.insert(id, status);
    }

    /// 更新任务
    pub fn update_task(&mut self, status: TaskStatus) {
        self.tasks.insert(status.id, status);
    }

    /// 移除任务
    pub fn remove_task(&mut self, id: TaskId) {
        self.tasks.remove(&id);
        self.order.retain(|&task_id| task_id != id);
    }

    /// 获取任务
    pub fn get_task(&self, id: &TaskId) -> Option<&TaskStatus> {
        self.tasks.get(id)
    }

    /// 获取所有任务（按顺序）
    pub fn iter_tasks(&self) -> impl Iterator<Item = &TaskStatus> {
        self.order.iter().filter_map(|id| self.tasks.get(id))
    }

    /// 任务数量
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// 清除已完成的任务
    pub fn clear_completed(&mut self) {
        let completed_ids: Vec<TaskId> = self
            .tasks
            .iter()
            .filter(|(_, status)| status.is_finished())
            .map(|(id, _)| *id)
            .collect();

        for id in completed_ids {
            self.remove_task(id);
        }
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> TaskStats {
        let mut stats = TaskStats::default();
        for status in self.tasks.values() {
            stats.total += 1;
            match &status.state {
                TaskState::Queued => stats.queued += 1,
                TaskState::Downloading => stats.downloading += 1,
                TaskState::Merging => stats.downloading += 1,
                TaskState::Paused => stats.paused += 1,
                TaskState::Completed => stats.completed += 1,
                TaskState::Failed(_) => stats.failed += 1,
                TaskState::Cancelled => stats.cancelled += 1,
            }
        }
        stats
    }
}

impl Default for TaskListData {
    fn default() -> Self {
        Self::new()
    }
}

/// 任务统计信息
#[derive(Debug, Clone, Default)]
pub struct TaskStats {
    pub total: usize,
    pub queued: usize,
    pub downloading: usize,
    pub paused: usize,
    pub completed: usize,
    pub failed: usize,
    pub cancelled: usize,
}

/// 任务列表视图组件
pub struct TaskListView {
    /// 任务数据
    data: Arc<RwLock<TaskListData>>,
    /// 操作回调
    _on_action: Option<TaskActionCallback>,
    /// 当前选中的任务
    selected_task: Option<TaskId>,
}

impl TaskListView {
    /// 创建新的任务列表视图
    pub fn new(data: Arc<RwLock<TaskListData>>) -> Self {
        Self {
            data,
            _on_action: None,
            selected_task: None,
        }
    }

    /// 设置操作回调
    pub fn on_action(mut self, callback: TaskActionCallback) -> Self {
        self._on_action = Some(callback);
        self
    }

    /// 选中任务
    pub fn select_task(&mut self, id: Option<TaskId>) {
        self.selected_task = id;
    }
}

impl Render for TaskListView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 尝试获取任务数据的快照用于渲染
        // 注意：这里使用 try_read 避免阻塞 UI 线程
        let tasks_snapshot: Vec<TaskStatus> = if let Ok(data) = self.data.try_read() {
            data.iter_tasks().cloned().collect()
        } else {
            Vec::new()
        };

        let stats = if let Ok(data) = self.data.try_read() {
            data.get_stats()
        } else {
            TaskStats::default()
        };

        let selected = self.selected_task;

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(render_toolbar(&stats, cx))
            .child(if tasks_snapshot.is_empty() {
                render_empty_state(cx).into_any_element()
            } else {
                render_task_list_content(&tasks_snapshot, selected, cx).into_any_element()
            })
    }
}

/// 渲染工具栏
fn render_toolbar(stats: &TaskStats, cx: &mut App) -> impl IntoElement {
    let theme = cx.theme();

    div()
        .h(px(48.0))
        .px(px(16.0))
        .border_b_1()
        .border_color(theme.border)
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.foreground)
                        .child(format!("任务列表 ({})", stats.total)),
                )
                .when(stats.downloading > 0, |this| {
                    this.child(
                        div()
                            .px(px(8.0))
                            .py(px(2.0))
                            .rounded(px(4.0))
                            .bg(theme.primary)
                            .text_xs()
                            .text_color(theme.primary_foreground)
                            .child(format!("{} 进行中", stats.downloading)),
                    )
                }),
        )
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .child(
                    Button::new("refresh")
                        .ghost()
                        .small()
                        .icon(IconName::Replace),
                )
                .child(
                    Button::new("clear-completed")
                        .ghost()
                        .small()
                        .label("清除已完成"),
                ),
        )
}

/// 渲染空状态
fn render_empty_state(cx: &mut App) -> impl IntoElement {
    let theme = cx.theme();

    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(8.0))
                .child(div().text_3xl().child("📭"))
                .child(
                    div()
                        .text_lg()
                        .text_color(theme.muted_foreground)
                        .child("暂无下载任务"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("在左侧面板添加视频链接开始下载"),
                ),
        )
}

/// 渲染任务列表内容
fn render_task_list_content(
    tasks: &[TaskStatus],
    selected: Option<TaskId>,
    cx: &mut App,
) -> impl IntoElement {
    let task_elements: Vec<AnyElement> = tasks
        .iter()
        .map(|task| render_task_item(task, selected == Some(task.id), cx).into_any_element())
        .collect();

    div().flex_1().overflow_hidden().child(
        div()
            .flex()
            .flex_col()
            .size_full()
            .p(px(16.0))
            .gap(px(8.0))
            .children(task_elements),
    )
}

/// 渲染单个任务项
fn render_task_item(task: &TaskStatus, is_selected: bool, cx: &mut App) -> impl IntoElement {
    let theme = cx.theme();
    let task_id = task.id;
    let state = task.state.clone();
    let progress = task.progress;
    let downloaded = task.downloaded_bytes;
    let total = task.total_bytes;
    let speed = task.speed;
    let eta = task.eta;
    let title = task.title.clone().unwrap_or_else(|| task.url.clone());
    // 手动截断标题，避免 GPUI DirectWrite 在 Windows 上的 UTF-8 边界 bug
    let title = truncate_string(&title, 40);
    let is_downloading = matches!(state, TaskState::Downloading | TaskState::Merging);
    let is_paused = matches!(state, TaskState::Paused);
    let is_failed = matches!(state, TaskState::Failed(_));
    let is_completed = matches!(state, TaskState::Completed);
    let is_active = task.is_active();

    // 预先创建进度条元素（避免借用问题）
    let progress_bar = if is_downloading {
        Some(render_progress_bar_element(progress, eta, theme))
    } else {
        None
    };

    // 提取需要的颜色值
    let border_color = if is_selected {
        theme.primary
    } else {
        theme.border
    };
    let bg_color = if is_selected {
        theme.accent
    } else {
        theme.secondary
    };
    let title_color = theme.foreground;
    let state_color_value = state_color(&state, theme);
    let info_color = theme.muted_foreground;

    div()
        .id(ElementId::Name(format!("task-{}", task_id).into()))
        .p(px(12.0))
        .border_1()
        .border_color(border_color)
        .rounded(px(8.0))
        .bg(bg_color)
        .flex()
        .flex_col()
        .gap(px(8.0))
        // 第一行：标题和状态
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(render_state_icon(&state))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(title_color)
                                .overflow_hidden()
                                .max_w(px(300.0))
                                .child(title),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(state_color_value)
                        .child(state_text(&state)),
                ),
        )
        // 第二行：进度条（如果正在下载）
        .when_some(progress_bar, |this, bar| this.child(bar))
        // 第三行：详情和操作按钮
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .text_xs()
                        .text_color(info_color)
                        .child(render_download_info(downloaded, total, speed)),
                )
                .child(render_action_buttons(
                    task_id,
                    is_downloading,
                    is_paused,
                    is_failed,
                    is_completed,
                    is_active,
                )),
        )
}

/// 渲染状态图标
fn render_state_icon(state: &TaskState) -> impl IntoElement {
    let icon_char = match state {
        TaskState::Queued => "⏳",
        TaskState::Downloading => "⬇️",
        TaskState::Merging => "🔄",
        TaskState::Paused => "⏸️",
        TaskState::Completed => "✅",
        TaskState::Failed(_) => "❌",
        TaskState::Cancelled => "🚫",
    };

    div().text_sm().child(icon_char.to_string())
}

/// 获取状态颜色
fn state_color(state: &TaskState, theme: &Theme) -> Hsla {
    match state {
        TaskState::Queued => theme.muted_foreground,
        TaskState::Downloading => theme.primary,
        TaskState::Merging => theme.warning,
        TaskState::Paused => theme.warning,
        TaskState::Completed => theme.success,
        TaskState::Failed(_) => theme.danger,
        TaskState::Cancelled => theme.muted_foreground,
    }
}

/// 获取状态文本
fn state_text(state: &TaskState) -> String {
    match state {
        TaskState::Queued => "排队中".to_string(),
        TaskState::Downloading => "下载中".to_string(),
        TaskState::Merging => "合并中".to_string(),
        TaskState::Paused => "已暂停".to_string(),
        TaskState::Completed => "已完成".to_string(),
        TaskState::Failed(err) => format!("失败: {}", err),
        TaskState::Cancelled => "已取消".to_string(),
    }
}

/// 渲染进度条元素（接受预先获取的 theme）
fn render_progress_bar_element(
    progress: f32,
    eta: Option<std::time::Duration>,
    theme: &Theme,
) -> impl IntoElement {
    let progress_clamped = progress.clamp(0.0, 100.0);
    let bg_color = theme.muted;
    let fg_color = theme.primary;
    let text_color = theme.muted_foreground;

    let eta_text = eta.map(|eta| format!("剩余 {}", format_duration(eta)));

    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            // 进度条背景
            div()
                .h(px(4.0))
                .rounded(px(2.0))
                .bg(bg_color)
                .overflow_hidden()
                .child(
                    // 进度条前景
                    div()
                        .h_full()
                        .rounded(px(2.0))
                        .bg(fg_color)
                        .w(relative(progress_clamped / 100.0)),
                ),
        )
        .child(
            // 进度文本
            div()
                .flex()
                .items_center()
                .justify_between()
                .text_xs()
                .text_color(text_color)
                .child(format!("{:.1}%", progress_clamped))
                .when_some(eta_text, |this, text| this.child(text)),
        )
}

/// 渲染下载信息
fn render_download_info(downloaded: u64, total: Option<u64>, speed: Option<u64>) -> String {
    let mut parts = Vec::new();

    // 已下载/总大小
    let downloaded_str = format_bytes(downloaded);
    if let Some(total) = total {
        parts.push(format!("{} / {}", downloaded_str, format_bytes(total)));
    } else {
        parts.push(downloaded_str);
    }

    // 下载速度
    if let Some(speed) = speed {
        parts.push(format!("{}/s", format_bytes(speed)));
    }

    parts.join(" • ")
}

/// 渲染操作按钮
fn render_action_buttons(
    task_id: TaskId,
    is_downloading: bool,
    is_paused: bool,
    is_failed: bool,
    is_completed: bool,
    is_active: bool,
) -> impl IntoElement {
    // 使用 task_id 的前 8 位作为唯一 ID
    let id_suffix = &task_id.to_string()[..8];

    div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .when(is_downloading, |this| {
            let btn_id: &str = Box::leak(format!("pause-{}", id_suffix).into_boxed_str());
            this.child(Button::new(btn_id).ghost().xsmall().icon(IconName::Minus))
        })
        .when(is_paused, |this| {
            let btn_id: &str = Box::leak(format!("resume-{}", id_suffix).into_boxed_str());
            this.child(
                Button::new(btn_id)
                    .ghost()
                    .xsmall()
                    .icon(IconName::ArrowRight),
            )
        })
        .when(is_failed, |this| {
            let btn_id: &str = Box::leak(format!("retry-{}", id_suffix).into_boxed_str());
            this.child(Button::new(btn_id).ghost().xsmall().icon(IconName::Replace))
        })
        .when(is_completed, |this| {
            let btn_id: &str = Box::leak(format!("open-{}", id_suffix).into_boxed_str());
            this.child(Button::new(btn_id).ghost().xsmall().icon(IconName::Folder))
        })
        .when(is_active, |this| {
            let btn_id: &str = Box::leak(format!("cancel-{}", id_suffix).into_boxed_str());
            this.child(Button::new(btn_id).ghost().xsmall().icon(IconName::Close))
        })
        .child({
            let btn_id: &str = Box::leak(format!("delete-{}", id_suffix).into_boxed_str());
            Button::new(btn_id).ghost().xsmall().icon(IconName::Delete)
        })
}

/// 格式化字节数
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// 格式化持续时间
fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let secs = secs % 60;

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, mins, secs)
    } else {
        format!("{}:{:02}", mins, secs)
    }
}

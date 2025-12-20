//! 任务列表页面主组件
//!
//! 设计原则：
//! - 只负责渲染，不直接管理下载逻辑
//! - 通过事件驱动更新任务状态
//! - 所有操作委托给 AppState

use crate::app::AppState;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::*;
use magekit_shared::{TaskId, TaskState, TaskStatus};
use std::sync::Arc;

use super::widgets::TaskItem;

/// 任务筛选类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskFilter {
    All,
    Downloading,
    Completed,
    Failed,
}

impl TaskFilter {
    pub fn label(&self) -> &'static str {
        match self {
            Self::All => "全部",
            Self::Downloading => "下载中",
            Self::Completed => "已完成",
            Self::Failed => "失败",
        }
    }
}

/// 任务列表页面
pub struct TasksPage {
    app_state: Arc<AppState>,
    tasks: Vec<TaskStatus>,
    filter: TaskFilter,
}

impl TasksPage {
    pub fn new(app_state: Arc<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        // 初始加载任务列表
        let tasks = app_state.get_all_tasks_sync();

        let page = Self {
            app_state: app_state.clone(),
            tasks,
            filter: TaskFilter::All,
        };

        // 事件驱动刷新：订阅 ToolManager 事件，收到更新后从 AppState 缓存读取最新任务列表
        // 额外加一个低频 tick，确保页面销毁后能及时退出循环（避免永久等待 recv）。
        let mut tool_manager_rx = app_state.tool_manager.subscribe();
        let app_state_for_events = app_state.clone();
        cx.spawn(async move |this, cx| {
            loop {
                tokio::select! {
                    evt = tool_manager_rx.recv() => {
                        match evt {
                            Ok(_) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                    _ = Timer::after(std::time::Duration::from_secs(1)) => {}
                }

                // 从 AppState 缓存读取最新任务
                let tasks: Vec<TaskStatus> = smol::unblock({
                    let app_state = app_state_for_events.clone();
                    move || app_state.get_all_tasks_sync()
                })
                .await;

                // 更新本地任务列表
                let should_continue = this.update(cx, |this, cx| {
                    // 检查是否有变化
                    let has_change = this.tasks.len() != tasks.len()
                        || this.tasks.iter().zip(tasks.iter()).any(|(a, b)| {
                            a.id != b.id
                                || a.progress != b.progress
                                || a.state != b.state
                                || a.speed != b.speed
                                || a.downloaded_bytes != b.downloaded_bytes
                        });

                    if has_change {
                        this.tasks = tasks;
                        cx.notify();
                    }
                    true
                });

                if should_continue.is_err() {
                    // 组件已销毁，退出循环
                    break;
                }
            }
        })
        .detach();

        page
    }

    /// 刷新任务列表
    fn refresh_tasks(&mut self, cx: &mut Context<Self>) {
        self.tasks = self.app_state.get_all_tasks_sync();
        cx.notify();
    }

    /// 设置筛选器
    fn set_filter(&mut self, filter: TaskFilter, cx: &mut Context<Self>) {
        self.filter = filter;
        cx.notify();
    }

    /// 获取筛选后的任务列表
    fn filtered_tasks(&self) -> Vec<&TaskStatus> {
        self.tasks
            .iter()
            .filter(|task| match self.filter {
                TaskFilter::All => true,
                TaskFilter::Downloading => {
                    matches!(
                        task.state,
                        TaskState::Downloading | TaskState::Paused | TaskState::Queued
                    )
                }
                TaskFilter::Completed => {
                    matches!(task.state, TaskState::Completed)
                }
                TaskFilter::Failed => {
                    matches!(task.state, TaskState::Failed(_) | TaskState::Cancelled)
                }
            })
            .collect()
    }

    /// 暂停任务
    fn pause_task(&mut self, task_id: TaskId, cx: &mut Context<Self>) {
        tracing::info!("⏸️ 暂停任务: {}", task_id);
        self.app_state.pause_download_sync(task_id);
        cx.notify();
    }

    /// 恢复任务
    fn resume_task(&mut self, task_id: TaskId, cx: &mut Context<Self>) {
        tracing::info!("▶️ 恢复任务: {}", task_id);
        self.app_state.resume_download_sync(task_id);
        cx.notify();
    }

    /// 取消任务
    fn cancel_task(&mut self, task_id: TaskId, cx: &mut Context<Self>) {
        tracing::info!("🛑 取消任务: {}", task_id);
        self.app_state.cancel_download_sync(task_id);
        cx.notify();
    }

    /// 删除任务
    fn delete_task(&mut self, task_id: TaskId, cx: &mut Context<Self>) {
        tracing::info!("🗑️ 删除任务: {}", task_id);
        self.app_state.delete_task_sync(task_id);

        // 立即从本地列表中移除
        self.tasks.retain(|t| t.id != task_id);
        cx.notify();
    }

    /// 打开文件夹
    fn open_folder(&self, task: &TaskStatus) {
        if let Some(path) = &task.output_path {
            if let Some(parent) = path.parent() {
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open").arg(parent).spawn();
                }
                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("explorer").arg(parent).spawn();
                }
                #[cfg(target_os = "linux")]
                {
                    let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
                }
            }
        }
    }

    /// 清空已完成的任务
    fn clear_completed(&mut self, cx: &mut Context<Self>) {
        self.app_state.clear_completed_tasks_sync();

        // 立即从本地列表中移除
        self.tasks
            .retain(|t| !matches!(t.state, TaskState::Completed));
        cx.notify();
    }
}

impl Render for TasksPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let filtered_tasks = self.filtered_tasks();
        let task_count = filtered_tasks.len();
        let is_empty = task_count == 0;

        // 使用主题颜色
        let bg_color = cx.theme().background;

        div()
            .id("tasks-page")
            .size_full()
            .overflow_y_scroll()
            .bg(bg_color)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p(px(24.0))
                    .gap(px(24.0))
                    // 页面标题
                    .child(self.render_header(cx))
                    // 筛选栏
                    .child(self.render_filter_bar(cx))
                    // 任务列表
                    .when(is_empty, |this| this.child(self.render_empty_state(cx)))
                    .when(!is_empty, |this| this.child(self.render_task_list(cx))),
            )
    }
}

impl TasksPage {
    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // 使用主题颜色
        let title_color = cx.theme().foreground;
        let desc_color = cx.theme().muted_foreground;

        div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::BOLD)
                            .text_color(title_color)
                            .child("📥 任务列表"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(desc_color)
                            .child(format!("共 {} 个任务", self.tasks.len())),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child({
                        let has_completed = self
                            .tasks
                            .iter()
                            .any(|t| matches!(t.state, TaskState::Completed));
                        Button::new("clear")
                            .xsmall()
                            .outline()
                            .disabled(!has_completed)
                            .label("清空已完成")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.clear_completed(cx);
                            }))
                    })
                    .child(
                        Button::new("refresh")
                            .xsmall()
                            .outline()
                            .label("刷新")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.refresh_tasks(cx);
                            })),
                    ),
            )
    }

    fn render_filter_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let current_filter = self.filter;
        let filters = [
            TaskFilter::All,
            TaskFilter::Downloading,
            TaskFilter::Completed,
            TaskFilter::Failed,
        ];

        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .children(filters.into_iter().map(|filter| {
                let is_active = filter == current_filter;
                let label = filter.label();

                Button::new(SharedString::from(format!("filter-{:?}", filter)))
                    .xsmall()
                    .map(|btn| {
                        if is_active {
                            btn.primary()
                        } else {
                            btn.outline()
                        }
                    })
                    .label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_filter(filter, cx);
                    }))
            }))
    }

    fn render_empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // 使用主题颜色
        let text_color = cx.theme().muted_foreground;
        let muted_color = cx.theme().muted_foreground;

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py(px(80.0))
            .gap(px(16.0))
            .child(div().text_2xl().child("📭"))
            .child(
                div()
                    .text_lg()
                    .text_color(text_color)
                    .child(match self.filter {
                        TaskFilter::All => "暂无下载任务",
                        TaskFilter::Downloading => "没有正在下载的任务",
                        TaskFilter::Completed => "没有已完成的任务",
                        TaskFilter::Failed => "没有失败的任务",
                    }),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted_color)
                    .child("在首页粘贴视频链接开始下载"),
            )
    }

    fn render_task_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let filtered_tasks = self.filtered_tasks();

        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .children(filtered_tasks.into_iter().map(|task| {
                let task_id = task.id;
                let task_clone = task.clone();

                TaskItem::new(task.clone())
                    .on_pause(cx.listener(move |this, _, _, cx| {
                        this.pause_task(task_id, cx);
                    }))
                    .on_resume(cx.listener(move |this, _, _, cx| {
                        this.resume_task(task_id, cx);
                    }))
                    .on_cancel(cx.listener(move |this, _, _, cx| {
                        this.cancel_task(task_id, cx);
                    }))
                    .on_delete(cx.listener(move |this, _, _, cx| {
                        this.delete_task(task_id, cx);
                    }))
                    .on_open_folder(cx.listener({
                        let task = task_clone.clone();
                        move |this, _, _, _| {
                            this.open_folder(&task);
                        }
                    }))
            }))
    }
}

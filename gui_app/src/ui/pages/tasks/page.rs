//! 任务列表页面主组件

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::*;
use gpui_component::button::{Button, ButtonVariants};
use crate::app::AppState;
use magekit_shared::{TaskStatus, TaskState, TaskId};
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
    is_loading: bool,
}

impl TasksPage {
    pub fn new(app_state: Arc<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        // 初始加载任务列表
        let tasks = Self::load_tasks_sync(&app_state);
        
        let page = Self {
            app_state: app_state.clone(),
            tasks,
            filter: TaskFilter::All,
            is_loading: false,
        };
        
        // 启动定时刷新任务（每秒刷新一次）
        let app_state_for_timer = app_state.clone();
        cx.spawn(async move |this, cx| {
            loop {
                // 等待 1 秒
                Timer::after(std::time::Duration::from_secs(1)).await;
                
                // 从 AppState 加载最新任务
                let app_state = app_state_for_timer.clone();
                let tasks: Vec<TaskStatus> = smol::unblock(move || {
                    let tasks = app_state.tasks.blocking_read();
                    tasks.values().cloned().collect()
                }).await;
                
                // 更新本地任务列表
                let should_continue = this.update(cx, |this, cx| {
                    // 只有任务列表有变化时才更新
                    if this.tasks.len() != tasks.len() || 
                       this.tasks.iter().zip(tasks.iter()).any(|(a, b)| {
                           a.id != b.id || a.progress != b.progress || a.state != b.state
                       }) {
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
        }).detach();
        
        page
    }

    /// 同步加载任务列表
    fn load_tasks_sync(app_state: &AppState) -> Vec<TaskStatus> {
        // 从 AppState 的 tasks HashMap 中获取所有任务
        app_state.runtime.block_on(async {
            let tasks = app_state.tasks.read().await;
            tasks.values().cloned().collect()
        })
    }

    /// 刷新任务列表
    fn refresh_tasks(&mut self, cx: &mut Context<Self>) {
        self.is_loading = true;
        cx.notify();
        
        let app_state = self.app_state.clone();
        
        cx.spawn(async move |this, cx| {
            // 使用 smol::unblock 避免阻塞
            let tasks: Vec<TaskStatus> = smol::unblock(move || {
                Self::load_tasks_sync(&app_state)
            }).await;
            
            let _ = this.update(cx, |this, cx| {
                this.tasks = tasks;
                this.is_loading = false;
                cx.notify();
            });
        }).detach();
    }

    /// 设置筛选器
    fn set_filter(&mut self, filter: TaskFilter, cx: &mut Context<Self>) {
        self.filter = filter;
        cx.notify();
    }

    /// 获取筛选后的任务列表
    fn filtered_tasks(&self) -> Vec<&TaskStatus> {
        self.tasks.iter().filter(|task| {
            match self.filter {
                TaskFilter::All => true,
                TaskFilter::Downloading => {
                    matches!(task.state, TaskState::Downloading | TaskState::Paused | TaskState::Queued)
                }
                TaskFilter::Completed => {
                    matches!(task.state, TaskState::Completed)
                }
                TaskFilter::Failed => {
                    matches!(task.state, TaskState::Failed(_) | TaskState::Cancelled)
                }
            }
        }).collect()
    }

    /// 暂停任务
    fn pause_task(&mut self, task_id: TaskId, cx: &mut Context<Self>) {
        tracing::info!("暂停任务: {}", task_id);
        
        // 更新本地状态
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.state = TaskState::Paused;
        }
        
        // 更新 AppState 中的任务状态
        let app_state = self.app_state.clone();
        cx.spawn(async move |this, cx| {
            smol::unblock(move || {
                let mut tasks = app_state.tasks.blocking_write();
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.state = TaskState::Paused;
                }
            }).await;
            
            let _ = this.update(cx, |_this, cx| {
                cx.notify();
            });
        }).detach();
        
        cx.notify();
    }

    /// 恢复任务
    fn resume_task(&mut self, task_id: TaskId, cx: &mut Context<Self>) {
        tracing::info!("恢复任务: {}", task_id);
        
        // 更新本地状态
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.state = TaskState::Downloading;
        }
        
        // 更新 AppState 中的任务状态
        let app_state = self.app_state.clone();
        cx.spawn(async move |this, cx| {
            smol::unblock(move || {
                let mut tasks = app_state.tasks.blocking_write();
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.state = TaskState::Downloading;
                }
            }).await;
            
            let _ = this.update(cx, |_this, cx| {
                cx.notify();
            });
        }).detach();
        
        cx.notify();
    }

    /// 取消任务
    fn cancel_task(&mut self, task_id: TaskId, cx: &mut Context<Self>) {
        tracing::info!("取消任务: {}", task_id);
        
        // 更新本地状态
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.state = TaskState::Cancelled;
            task.completed_at = Some(std::time::SystemTime::now());
        }
        
        // 更新 AppState 中的任务状态
        let app_state = self.app_state.clone();
        cx.spawn(async move |this, cx| {
            smol::unblock(move || {
                let mut tasks = app_state.tasks.blocking_write();
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.state = TaskState::Cancelled;
                    task.completed_at = Some(std::time::SystemTime::now());
                }
            }).await;
            
            let _ = this.update(cx, |_this, cx| {
                cx.notify();
            });
        }).detach();
        
        cx.notify();
    }

    /// 删除任务
    fn delete_task(&mut self, task_id: TaskId, cx: &mut Context<Self>) {
        tracing::info!("删除任务: {}", task_id);
        
        // 从本地列表中移除
        self.tasks.retain(|t| t.id != task_id);
        
        // 从 AppState 中移除
        let app_state = self.app_state.clone();
        cx.spawn(async move |_this, _cx| {
            smol::unblock(move || {
                let mut tasks = app_state.tasks.blocking_write();
                tasks.remove(&task_id);
            }).await;
        }).detach();
        
        cx.notify();
    }

    /// 打开文件夹
    fn open_folder(&self, task: &TaskStatus) {
        if let Some(path) = &task.output_path {
            if let Some(parent) = path.parent() {
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open")
                        .arg(parent)
                        .spawn();
                }
                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("explorer")
                        .arg(parent)
                        .spawn();
                }
                #[cfg(target_os = "linux")]
                {
                    let _ = std::process::Command::new("xdg-open")
                        .arg(parent)
                        .spawn();
                }
            }
        }
    }

    /// 清空已完成的任务
    fn clear_completed(&mut self, cx: &mut Context<Self>) {
        // 获取要删除的任务 ID
        let completed_ids: Vec<TaskId> = self.tasks.iter()
            .filter(|t| matches!(t.state, TaskState::Completed))
            .map(|t| t.id)
            .collect();
        
        // 从本地列表中移除
        self.tasks.retain(|t| !matches!(t.state, TaskState::Completed));
        
        // 从 AppState 中移除
        if !completed_ids.is_empty() {
            let app_state = self.app_state.clone();
            cx.spawn(async move |_this, _cx| {
                smol::unblock(move || {
                    let mut tasks = app_state.tasks.blocking_write();
                    for id in completed_ids {
                        tasks.remove(&id);
                    }
                }).await;
            }).detach();
        }
        
        cx.notify();
    }
}

impl Render for TasksPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let filtered_tasks = self.filtered_tasks();
        let task_count = filtered_tasks.len();
        let is_empty = task_count == 0;

        div()
            .id("tasks-page")
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(0x09090b))
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
                    .when(is_empty, |this| {
                        this.child(self.render_empty_state())
                    })
                    .when(!is_empty, |this| {
                        this.child(self.render_task_list(cx))
                    })
            )
    }
}

impl TasksPage {
    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
                            .text_color(rgb(0xfafafa))
                            .child("📥 任务列表")
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xa1a1aa))
                            .child(format!("共 {} 个任务", self.tasks.len()))
                    )
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child({
                        let has_completed = self.tasks.iter().any(|t| matches!(t.state, TaskState::Completed));
                        Button::new("clear")
                            .xsmall()
                            .ghost()
                            .disabled(!has_completed)
                            .label("清空已完成")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.clear_completed(cx);
                            }))
                    })
                    .child(
                        Button::new("refresh")
                            .xsmall()
                            .ghost()
                            .label("刷新")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.refresh_tasks(cx);
                            }))
                    )
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
                            btn.ghost()
                        }
                    })
                    .label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_filter(filter, cx);
                    }))
            }))
    }

    fn render_empty_state(&self) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py(px(80.0))
            .gap(px(16.0))
            .child(
                div()
                    .text_2xl()
                    .child("📭")
            )
            .child(
                div()
                    .text_lg()
                    .text_color(rgb(0xa1a1aa))
                    .child(match self.filter {
                        TaskFilter::All => "暂无下载任务",
                        TaskFilter::Downloading => "没有正在下载的任务",
                        TaskFilter::Completed => "没有已完成的任务",
                        TaskFilter::Failed => "没有失败的任务",
                    })
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x71717a))
                    .child("在首页粘贴视频链接开始下载")
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

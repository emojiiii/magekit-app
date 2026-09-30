//! 任务列表页面主组件
//!
//! 设计原则：
//! - 只负责渲染，不直接管理下载逻辑
//! - 通过事件驱动更新任务状态
//! - 所有操作委托给 AppState

use crate::app::AppState;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::empty::{Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::*;
use gpui_router::use_navigate;
use magekit_shared::{TaskId, TaskState, TaskStatus};
use std::{collections::HashSet, sync::Arc};

use super::widgets::TaskItem;

/// 任务筛选类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskFilter {
    All,
    Downloading,
    Completed,
    Failed,
    Cancelled,
}

impl TaskFilter {
    pub fn label(&self) -> &'static str {
        match self {
            Self::All => crate::i18n::tr("全部"),
            Self::Downloading => crate::i18n::tr("进行中"),
            Self::Completed => crate::i18n::tr("已完成"),
            Self::Failed => crate::i18n::tr("失败"),
            Self::Cancelled => crate::i18n::tr("已取消"),
        }
    }
}

#[derive(Clone, Copy)]
enum TaskAction {
    Pause,
    Resume,
    Retry,
    Cancel,
    Delete,
}

impl TaskAction {
    fn allowed(self, state: &TaskState) -> bool {
        match self {
            Self::Pause => matches!(state, TaskState::Downloading),
            Self::Resume => matches!(state, TaskState::Paused),
            Self::Retry => matches!(state, TaskState::Failed(_)),
            Self::Cancel => matches!(
                state,
                TaskState::Queued | TaskState::Downloading | TaskState::Paused | TaskState::Merging
            ),
            Self::Delete => matches!(
                state,
                TaskState::Completed | TaskState::Failed(_) | TaskState::Cancelled
            ),
        }
    }
}

/// 任务列表页面
pub struct TasksPage {
    app_state: Arc<AppState>,
    tasks: Vec<TaskStatus>,
    filter: TaskFilter,
    pending_tasks: HashSet<TaskId>,
    clearing: bool,
}

impl TasksPage {
    pub fn new(app_state: Arc<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        // 初始加载任务列表
        let tasks = app_state.get_all_tasks_sync();

        let page = Self {
            app_state: app_state.clone(),
            tasks,
            filter: TaskFilter::All,
            pending_tasks: HashSet::new(),
            clearing: false,
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
                    _ = smol::Timer::after(std::time::Duration::from_secs(1)) => {}
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
                                || a.total_bytes != b.total_bytes
                                || a.title != b.title
                                || a.output_path != b.output_path
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
                        TaskState::Downloading
                            | TaskState::Merging
                            | TaskState::Paused
                            | TaskState::Queued
                    )
                }
                TaskFilter::Completed => {
                    matches!(task.state, TaskState::Completed)
                }
                TaskFilter::Failed => matches!(task.state, TaskState::Failed(_)),
                TaskFilter::Cancelled => matches!(task.state, TaskState::Cancelled),
            })
            .collect()
    }

    /// 重复点击只发出一次请求；错误保持可见并允许再次操作。
    fn run_task_action(
        &mut self,
        id: TaskId,
        action: TaskAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.clearing || self.pending_tasks.contains(&id) {
            return;
        }
        let Some(task) = self.app_state.get_task_status_sync(id) else {
            return;
        };
        if !action.allowed(&task.state) {
            self.refresh_tasks(cx);
            return;
        }
        self.pending_tasks.insert(id);
        let handle = match action {
            TaskAction::Pause => self.app_state.pause_download_sync(id),
            TaskAction::Resume => self.app_state.resume_download_sync(id),
            TaskAction::Retry => self.app_state.retry_task(id),
            TaskAction::Cancel => self.app_state.cancel_download_sync(id),
            TaskAction::Delete => self.app_state.delete_task_sync(id),
        };
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = handle
                .await
                .map_err(anyhow::Error::from)
                .and_then(|result| result);
            let _ = this.update_in(cx, |this, window, cx| {
                this.pending_tasks.remove(&id);
                this.refresh_tasks(cx);
                if let Err(error) = result {
                    window.push_notification(
                        Notification::error(crate::i18n::format(
                            "任务操作失败: {}",
                            &[error.to_string()],
                        )),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    fn confirm_task_action(
        &self,
        id: TaskId,
        action: TaskAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.clearing || self.pending_tasks.contains(&id) {
            return;
        }
        let Some(task) = self.tasks.iter().find(|task| task.id == id) else {
            return;
        };
        let title = task.title.clone().unwrap_or_else(|| task.url.clone());
        let (heading, description, action_label) = match action {
            TaskAction::Cancel => (
                "取消下载？",
                "取消后将停止此任务，可从首页重新添加。",
                "取消下载",
            ),
            _ => (
                "删除任务记录？",
                "这将删除任务记录和临时文件，已下载的文件会保留。",
                "删除",
            ),
        };
        let page = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let page = page.clone();
            dialog
                .confirm()
                .title(crate::i18n::tr(heading))
                .description(format!("{}\n{}", title, crate::i18n::tr(description)))
                .ok_text(crate::i18n::tr(action_label))
                .cancel_text(crate::i18n::tr("返回"))
                .ok_variant(gpui_kit::component::button::ButtonVariant::Danger)
                .on_ok(move |_, window, cx| {
                    let _ =
                        page.update(cx, |this, cx| this.run_task_action(id, action, window, cx));
                    true
                })
        });
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

    /// 清理前确认范围，完成后再移除记录。
    fn confirm_clear_completed(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.clearing || !self.pending_tasks.is_empty() {
            return;
        }
        let completed_ids: Vec<_> = self
            .tasks
            .iter()
            .filter(|task| matches!(task.state, TaskState::Completed))
            .map(|task| task.id)
            .collect();
        let count = completed_ids.len();
        let page = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let page = page.clone();
            let completed_ids = completed_ids.clone();
            dialog
                .confirm()
                .title(crate::i18n::tr("清空已完成的任务？"))
                .description(crate::i18n::format(
                    "将删除 {} 条已完成记录，下载的文件会保留。",
                    &[count.to_string()],
                ))
                .ok_text(crate::i18n::tr("清空已完成"))
                .cancel_text(crate::i18n::tr("返回"))
                .ok_variant(gpui_kit::component::button::ButtonVariant::Danger)
                .on_ok(move |_, window, cx| {
                    let _ = page.update(cx, |this, cx| {
                        this.clear_completed(completed_ids.clone(), window, cx)
                    });
                    true
                })
        });
    }

    fn clear_completed(
        &mut self,
        completed_ids: Vec<TaskId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.clearing || !self.pending_tasks.is_empty() {
            return;
        }
        self.clearing = true;
        let handle = self.app_state.clear_completed_tasks_sync(completed_ids);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = handle
                .await
                .map_err(anyhow::Error::from)
                .and_then(|result| result);
            let _ = this.update_in(cx, |this, window, cx| {
                this.clearing = false;
                this.refresh_tasks(cx);
                if let Err(error) = result {
                    window.push_notification(
                        Notification::error(crate::i18n::format(
                            "清理任务失败: {}",
                            &[error.to_string()],
                        )),
                        cx,
                    );
                }
            });
        })
        .detach();
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
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .p_6()
                    .gap_4()
                    .child(self.render_header(cx))
                    .child(self.render_filter_bar(cx))
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
            .flex_wrap()
            .gap_3()
            .pb_4()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(22.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(title_color)
                            .child(crate::i18n::tr("下载任务")),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(desc_color)
                            .child(crate::i18n::tr("管理所有下载任务")),
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
                            .small()
                            .ghost()
                            .disabled(
                                !has_completed || self.clearing || !self.pending_tasks.is_empty(),
                            )
                            .loading(self.clearing)
                            .label(crate::i18n::tr("清空已完成"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.confirm_clear_completed(window, cx);
                            }))
                    })
                    .child(
                        Button::new("refresh")
                            .small()
                            .outline()
                            .icon(IconName::RefreshCw)
                            .label(crate::i18n::tr("刷新"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.refresh_tasks(cx);
                            })),
                    ),
            )
    }

    fn filter_count(&self, filter: TaskFilter) -> usize {
        self.tasks
            .iter()
            .filter(|task| match filter {
                TaskFilter::All => true,
                TaskFilter::Downloading => matches!(
                    task.state,
                    TaskState::Downloading
                        | TaskState::Merging
                        | TaskState::Paused
                        | TaskState::Queued
                ),
                TaskFilter::Completed => matches!(task.state, TaskState::Completed),
                TaskFilter::Failed => matches!(task.state, TaskState::Failed(_)),
                TaskFilter::Cancelled => matches!(task.state, TaskState::Cancelled),
            })
            .count()
    }

    fn render_filter_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let filters = [
            TaskFilter::All,
            TaskFilter::Downloading,
            TaskFilter::Completed,
            TaskFilter::Failed,
            TaskFilter::Cancelled,
        ];
        let selected = filters
            .iter()
            .position(|filter| *filter == self.filter)
            .unwrap_or(0);
        TabBar::new("download-filters")
            .underline()
            .menu(true)
            .selected_index(selected)
            .children(filters.into_iter().map(|filter| {
                Tab::new().label(format!("{}  {}", filter.label(), self.filter_count(filter)))
            }))
            .on_click(cx.listener(move |this, index: &usize, _, cx| {
                if let Some(filter) = filters.get(*index) {
                    this.set_filter(*filter, cx);
                }
            }))
    }

    fn render_empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        Empty::new()
            .py_6()
            .header(
                EmptyHeader::new()
                    .media(
                        EmptyMedia::new()
                            .size_8()
                            .rounded_xl()
                            .bg(cx.theme().muted)
                            .child(
                                Icon::new(IconName::Inbox)
                                    .size_4()
                                    .text_color(cx.theme().muted_foreground),
                            ),
                    )
                    .title(EmptyTitle::new().child(match self.filter {
                        TaskFilter::All => crate::i18n::tr("暂无下载任务"),
                        TaskFilter::Downloading => crate::i18n::tr("没有正在下载的任务"),
                        TaskFilter::Completed => crate::i18n::tr("没有已完成的任务"),
                        TaskFilter::Failed => crate::i18n::tr("没有失败的任务"),
                        TaskFilter::Cancelled => crate::i18n::tr("没有已取消的任务"),
                    }))
                    .description(
                        EmptyDescription::new()
                            .child(crate::i18n::tr("在首页粘贴视频链接开始下载")),
                    ),
            )
            .child(
                Button::new("new-download")
                    .primary()
                    .icon(IconName::Plus)
                    .label(crate::i18n::tr("新建下载"))
                    .on_click(cx.listener(|_, _, _, cx| {
                        use_navigate(cx)("/".into());
                    })),
            )
    }

    fn render_task_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let filtered_tasks = self.filtered_tasks();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .children(filtered_tasks.into_iter().map(|task| {
                let task_id = task.id;
                let task_clone = task.clone();

                TaskItem::new(task.clone())
                    .pending(self.clearing || self.pending_tasks.contains(&task_id))
                    .on_pause(cx.listener(move |this, _, window, cx| {
                        this.run_task_action(task_id, TaskAction::Pause, window, cx);
                    }))
                    .on_resume(cx.listener(move |this, _, window, cx| {
                        this.run_task_action(task_id, TaskAction::Resume, window, cx);
                    }))
                    .on_retry(cx.listener(move |this, _, window, cx| {
                        this.run_task_action(task_id, TaskAction::Retry, window, cx);
                    }))
                    .on_cancel(cx.listener(move |this, _, window, cx| {
                        this.confirm_task_action(task_id, TaskAction::Cancel, window, cx);
                    }))
                    .on_delete(cx.listener(move |this, _, window, cx| {
                        this.confirm_task_action(task_id, TaskAction::Delete, window, cx);
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

#[cfg(test)]
mod tests {
    use super::TaskAction;
    use magekit_shared::TaskState;

    #[test]
    fn late_confirmation_cannot_cancel_a_completed_task() {
        assert!(!TaskAction::Cancel.allowed(&TaskState::Completed));
        assert!(TaskAction::Cancel.allowed(&TaskState::Queued));
        assert!(!TaskAction::Delete.allowed(&TaskState::Downloading));
    }

    #[test]
    fn retry_and_resume_have_distinct_states() {
        assert!(TaskAction::Retry.allowed(&TaskState::Failed("network".into())));
        assert!(!TaskAction::Resume.allowed(&TaskState::Failed("network".into())));
        assert!(TaskAction::Resume.allowed(&TaskState::Paused));
    }
}

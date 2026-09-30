//! 任务项组件

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants};
use gpui_kit::component::progress::Progress;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::*;
use magekit_shared::{TaskState, TaskStatus, truncate_string};
use std::sync::Arc;

/// 任务项组件
#[derive(IntoElement)]
pub struct TaskItem {
    task: TaskStatus,
    pending: bool,
    on_retry: Option<Arc<dyn Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync>>,
    on_pause: Option<Arc<dyn Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync>>,
    on_resume: Option<Arc<dyn Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync>>,
    on_cancel: Option<Arc<dyn Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync>>,
    on_delete: Option<Arc<dyn Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync>>,
    on_open_folder: Option<Arc<dyn Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync>>,
}

impl TaskItem {
    pub fn new(task: TaskStatus) -> Self {
        Self {
            task,
            pending: false,
            on_retry: None,
            on_pause: None,
            on_resume: None,
            on_cancel: None,
            on_delete: None,
            on_open_folder: None,
        }
    }

    pub fn pending(mut self, pending: bool) -> Self {
        self.pending = pending;
        self
    }

    pub fn on_retry(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync + 'static,
    ) -> Self {
        self.on_retry = Some(Arc::new(handler));
        self
    }

    pub fn on_pause(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync + 'static,
    ) -> Self {
        self.on_pause = Some(Arc::new(handler));
        self
    }

    pub fn on_resume(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync + 'static,
    ) -> Self {
        self.on_resume = Some(Arc::new(handler));
        self
    }

    pub fn on_cancel(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync + 'static,
    ) -> Self {
        self.on_cancel = Some(Arc::new(handler));
        self
    }

    pub fn on_delete(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync + 'static,
    ) -> Self {
        self.on_delete = Some(Arc::new(handler));
        self
    }

    pub fn on_open_folder(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync + 'static,
    ) -> Self {
        self.on_open_folder = Some(Arc::new(handler));
        self
    }
}

impl RenderOnce for TaskItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let task = &self.task;
        let task_id = task.id;

        // 使用主题颜色
        let border_color = cx.theme().border;
        let title_color = cx.theme().foreground;
        let muted_color = cx.theme().muted_foreground;
        let error_color = cx.theme().danger;
        let pending = self.pending;
        let on_retry = self.on_retry;

        // 状态图标和颜色
        let (status_icon, status_color, status_text) = match &task.state {
            TaskState::Queued => (
                IconName::LoaderCircle,
                cx.theme().warning,
                crate::i18n::tr("等待中").to_string(),
            ),
            TaskState::Downloading => (
                IconName::ArrowDown,
                cx.theme().primary,
                crate::i18n::tr("下载中").to_string(),
            ),
            TaskState::Merging => (
                IconName::RefreshCw,
                cx.theme().primary,
                crate::i18n::tr("合并中").to_string(),
            ),
            TaskState::Paused => (
                IconName::Pause,
                cx.theme().warning,
                crate::i18n::tr("已暂停").to_string(),
            ),
            TaskState::Completed => (
                IconName::CircleCheck,
                cx.theme().success,
                crate::i18n::tr("已完成").to_string(),
            ),
            TaskState::Failed(_) => (
                IconName::CircleAlert,
                cx.theme().danger,
                crate::i18n::tr("失败").to_string(),
            ),
            TaskState::Cancelled => (
                IconName::Ban,
                cx.theme().muted_foreground,
                crate::i18n::tr("已取消").to_string(),
            ),
        };

        // 获取失败原因
        let error_message = match &task.state {
            TaskState::Failed(msg) => Some(msg.clone()),
            _ => None,
        };
        let is_failed = error_message.is_some();

        // 计算进度百分比
        let progress_percent = if task.progress.is_finite() {
            task.progress.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let _progress_width = format!("{}%", (progress_percent * 100.0) as i32);

        // 格式化速度
        let speed_str = task.speed.map(|s| format_speed(s)).unwrap_or_default();

        // 格式化大小
        let size_str = if let Some(total) = task.total_bytes {
            format!(
                "{} / {}",
                format_bytes(task.downloaded_bytes),
                format_bytes(total)
            )
        } else if task.downloaded_bytes > 0 {
            format_bytes(task.downloaded_bytes)
        } else {
            crate::i18n::tr("计算中...").to_string()
        };

        // 标题（使用 URL 的最后部分作为备用）
        // 手动截断标题，避免 GPUI DirectWrite 在 Windows 上的 UTF-8 边界 bug
        // 使用较短的截断长度，因为 GPUI 可能会因为宽度限制再次截断
        let title = task.title.clone().unwrap_or_else(|| {
            task.url
                .split('/')
                .last()
                .unwrap_or(crate::i18n::tr("未知"))
                .to_string()
        });
        let title = truncate_string(&title, 50);

        let on_pause = self.on_pause;
        let on_resume = self.on_resume;
        let on_cancel = self.on_cancel;
        let on_delete = self.on_delete;
        let on_open_folder = self.on_open_folder;
        let is_downloading = matches!(task.state, TaskState::Downloading);
        let is_paused = matches!(task.state, TaskState::Paused);
        let is_completed = matches!(task.state, TaskState::Completed);
        let is_active = matches!(
            task.state,
            TaskState::Downloading | TaskState::Merging | TaskState::Paused | TaskState::Queued
        );

        GroupBox::new()
            .id(SharedString::from(format!("task-card-{task_id}")))
            .fill()
            .content_style(StyleRefinement::default().p_5().gap_3().rounded_lg())
            // 顶部：标题和状态
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0() // 防止标题撑开容器
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .size_9()
                                    .rounded_lg()
                                    .bg(status_color.opacity(0.1))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        Icon::new(status_icon).size_4().text_color(status_color),
                                    ),
                            )
                            .child(
                                div().flex_1().min_w_0().overflow_hidden().child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(title_color)
                                        .overflow_hidden()
                                        .child(title),
                                ),
                            ),
                    )
                    .child(
                        match &task.state {
                            TaskState::Completed => Tag::success(),
                            TaskState::Failed(_) => Tag::danger(),
                            TaskState::Paused | TaskState::Queued => Tag::warning(),
                            TaskState::Cancelled => Tag::secondary(),
                            _ => Tag::primary(),
                        }
                        .small()
                        .outline()
                        .child(status_text),
                    ),
            )
            // 失败原因显示
            .when_some(error_message, |this, msg| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(error_color)
                        .p(px(8.0))
                        .bg(error_color.opacity(0.1))
                        .rounded(px(4.0))
                        .child(crate::i18n::format("原因: {}", &[format!("{}", msg)])),
                )
            })
            // Kit 进度组件提供主题、无障碍标签和数值边界。
            .when(is_active, |this| {
                this.child(
                    Progress::new(SharedString::from(format!("progress-{}", task_id)))
                        .value(progress_percent * 100.0)
                        .loading(matches!(task.state, TaskState::Queued | TaskState::Merging))
                        .accessibility_label(crate::i18n::tr("下载进度")),
                )
            })
            // 信息行
            .when(!is_failed, |this| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .text_xs()
                        .text_color(muted_color)
                        .child(size_str.clone())
                        .when(is_downloading, |this| this.child(speed_str.clone()))
                        .when(is_active, |this| {
                            this.child(format!("{:.1}%", progress_percent * 100.0))
                        }),
                )
            })
            // 操作按钮
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .pt_3()
                    .border_t_1()
                    .border_color(border_color)
                    .justify_end()
                    .flex_wrap()
                    .when(pending, |row| {
                        row.child(
                            div()
                                .text_xs()
                                .text_color(muted_color)
                                .child(crate::i18n::tr("处理中...")),
                        )
                    })
                    .when(is_failed, |row| {
                        row.child(
                            Button::new(SharedString::from(format!("retry-{}", task_id)))
                                .small()
                                .primary()
                                .disabled(pending)
                                .label(crate::i18n::tr("重试"))
                                .when_some(on_retry, |button, handler| {
                                    button.on_click(move |event, window, cx| {
                                        handler(event, window, cx)
                                    })
                                }),
                        )
                    })
                    // 暂停按钮（下载中显示）
                    .when(is_downloading, |this| {
                        let handler = on_pause.clone();
                        // 使用任务 ID 作为按钮 ID 的一部分，确保唯一性
                        let btn_id = SharedString::from(format!("pause-{}", task_id));
                        this.child(
                            Button::new(btn_id)
                                .small()
                                .disabled(pending)
                                .outline()
                                .label(crate::i18n::tr("暂停"))
                                .when_some(handler, |btn, h| {
                                    btn.on_click(move |e, w, cx| h(e, w, cx))
                                }),
                        )
                    })
                    // 继续按钮（暂停状态显示）
                    .when(is_paused, |this| {
                        let handler = on_resume.clone();
                        let btn_id = SharedString::from(format!("resume-{}", task_id));
                        this.child(
                            Button::new(btn_id)
                                .small()
                                .disabled(pending)
                                .primary()
                                .label(crate::i18n::tr("继续"))
                                .when_some(handler, |btn, h| {
                                    btn.on_click(move |e, w, cx| h(e, w, cx))
                                }),
                        )
                    })
                    // 取消按钮（活动状态显示）
                    .when(is_active, |this| {
                        let handler = on_cancel.clone();
                        let btn_id = SharedString::from(format!("cancel-{}", task_id));
                        this.child(
                            Button::new(btn_id)
                                .small()
                                .disabled(pending)
                                .ghost()
                                .text_color(error_color)
                                .label(crate::i18n::tr("取消"))
                                .when_some(handler, |btn, h| {
                                    btn.on_click(move |e, w, cx| h(e, w, cx))
                                }),
                        )
                    })
                    // 打开文件夹按钮
                    .when(is_completed, |this| {
                        let handler = on_open_folder.clone();
                        let btn_id = SharedString::from(format!("open-{}", task_id));
                        this.child(
                            Button::new(btn_id)
                                .small()
                                .disabled(pending)
                                .outline()
                                .label(crate::i18n::tr("打开文件夹"))
                                .when_some(handler, |btn, h| {
                                    btn.on_click(move |e, w, cx| h(e, w, cx))
                                }),
                        )
                    })
                    // 删除按钮（非活动任务）
                    .when(!is_active, |this| {
                        let handler = on_delete.clone();
                        let btn_id = SharedString::from(format!("delete-{}", task_id));
                        this.child(
                            Button::new(btn_id)
                                .small()
                                .disabled(pending)
                                .ghost()
                                .text_color(error_color)
                                .label(crate::i18n::tr("删除"))
                                .when_some(handler, |btn, h| {
                                    btn.on_click(move |e, w, cx| h(e, w, cx))
                                }),
                        )
                    }),
            )
    }
}

/// 格式化字节数
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

/// 格式化下载速度
fn format_speed(bytes_per_sec: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;

    if bytes_per_sec >= MB {
        format!("{:.2} MB/s", bytes_per_sec as f64 / MB as f64)
    } else if bytes_per_sec >= KB {
        format!("{:.2} KB/s", bytes_per_sec as f64 / KB as f64)
    } else {
        format!("{} B/s", bytes_per_sec)
    }
}

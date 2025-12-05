//! 任务项组件

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::*;
use gpui_component::button::{Button, ButtonVariants};
use magekit_shared::{TaskStatus, TaskState};
use std::sync::Arc;

/// 任务项组件
#[derive(IntoElement)]
pub struct TaskItem {
    task: TaskStatus,
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
            on_pause: None,
            on_resume: None,
            on_cancel: None,
            on_delete: None,
            on_open_folder: None,
        }
    }

    pub fn on_pause(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync + 'static) -> Self {
        self.on_pause = Some(Arc::new(handler));
        self
    }

    pub fn on_resume(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync + 'static) -> Self {
        self.on_resume = Some(Arc::new(handler));
        self
    }

    pub fn on_cancel(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync + 'static) -> Self {
        self.on_cancel = Some(Arc::new(handler));
        self
    }

    pub fn on_delete(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync + 'static) -> Self {
        self.on_delete = Some(Arc::new(handler));
        self
    }

    pub fn on_open_folder(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + Send + Sync + 'static) -> Self {
        self.on_open_folder = Some(Arc::new(handler));
        self
    }
}

impl RenderOnce for TaskItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let task = &self.task;
        
        // 检测当前是否是暗色模式
        let is_dark = cx.theme().mode.is_dark();
        
        // 根据主题选择颜色
        let bg_color = if is_dark { rgb(0x18181b) } else { rgb(0xffffff) };
        let border_color = if is_dark { rgb(0x3f3f46) } else { rgb(0xe4e4e7) };
        let title_color = if is_dark { rgb(0xfafafa) } else { rgb(0x18181b) };
        let muted_color = if is_dark { rgb(0x71717a) } else { rgb(0xa1a1aa) };
        let progress_bg = if is_dark { rgb(0x27272a) } else { rgb(0xf4f4f5) };
        
        // 状态图标和颜色
        let (status_icon, status_color, status_text) = match &task.state {
            TaskState::Queued => ("⏳", rgb(0xfbbf24), "等待中"),
            TaskState::Downloading => ("⬇️", rgb(0x3b82f6), "下载中"),
            TaskState::Paused => ("⏸️", rgb(0xf59e0b), "已暂停"),
            TaskState::Completed => ("✅", rgb(0x22c55e), "已完成"),
            TaskState::Failed(_) => ("❌", rgb(0xef4444), "失败"),
            TaskState::Cancelled => ("🚫", rgb(0x6b7280), "已取消"),
        };

        // 计算进度百分比
        let progress_percent = task.progress;
        let _progress_width = format!("{}%", (progress_percent * 100.0) as i32);

        // 格式化速度
        let speed_str = task.speed.map(|s| format_speed(s)).unwrap_or_default();

        // 格式化大小
        let size_str = if let Some(total) = task.total_bytes {
            format!("{} / {}", format_bytes(task.downloaded_bytes), format_bytes(total))
        } else if task.downloaded_bytes > 0 {
            format_bytes(task.downloaded_bytes)
        } else {
            "计算中...".to_string()
        };

        // 标题（使用 URL 的最后部分作为备用）
        let title = task.title.clone().unwrap_or_else(|| {
            task.url.split('/').last().unwrap_or("未知").to_string()
        });

        let on_pause = self.on_pause;
        let on_resume = self.on_resume;
        let on_cancel = self.on_cancel;
        let on_delete = self.on_delete;
        let on_open_folder = self.on_open_folder;
        let is_downloading = matches!(task.state, TaskState::Downloading);
        let is_paused = matches!(task.state, TaskState::Paused);
        let is_completed = matches!(task.state, TaskState::Completed);
        let is_active = matches!(task.state, TaskState::Downloading | TaskState::Paused | TaskState::Queued);

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(16.0))
            .bg(bg_color)
            .border_1()
            .border_color(border_color)
            .rounded(px(12.0))
            // 顶部：标题和状态
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
                            .child(div().text_lg().child(status_icon))
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(title_color)
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child(title)
                                    )
                            )
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(status_color)
                            .child(status_text)
                    )
            )
            // 进度条
            .when(is_active, |this| {
                this.child(
                    div()
                        .h(px(4.0))
                        .w_full()
                        .bg(progress_bg)
                        .rounded(px(2.0))
                        .child(
                            div()
                                .h_full()
                                .rounded(px(2.0))
                                .bg(rgb(0x3b82f6))
                                .w(relative(progress_percent))
                        )
                )
            })
            // 信息行
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_xs()
                    .text_color(muted_color)
                    .child(size_str)
                    .when(is_downloading, |this| {
                        this.child(speed_str)
                    })
                    .when(is_active, |this| {
                        this.child(format!("{:.1}%", progress_percent * 100.0))
                    })
            )
            // 操作按钮
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .pt(px(8.0))
                    // 暂停按钮
                    .when(is_downloading, |this| {
                        let handler = on_pause.clone();
                        this.child(
                            Button::new("pause")
                                .xsmall()
                                .outline()
                                .label("暂停")
                                .when_some(handler, |btn, h| {
                                    btn.on_click(move |e, w, cx| h(e, w, cx))
                                })
                        )
                    })
                    // 继续按钮
                    .when(is_paused, |this| {
                        let handler = on_resume.clone();
                        this.child(
                            Button::new("resume")
                                .xsmall()
                                .primary()
                                .label("继续")
                                .when_some(handler, |btn, h| {
                                    btn.on_click(move |e, w, cx| h(e, w, cx))
                                })
                        )
                    })
                    // 取消按钮
                    .when(is_active, |this| {
                        let handler = on_cancel.clone();
                        this.child(
                            Button::new("cancel")
                                .xsmall()
                                .danger()
                                .label("取消")
                                .when_some(handler, |btn, h| {
                                    btn.on_click(move |e, w, cx| h(e, w, cx))
                                })
                        )
                    })
                    // 打开文件夹按钮
                    .when(is_completed, |this| {
                        let handler = on_open_folder.clone();
                        this.child(
                            Button::new("open")
                                .xsmall()
                                .outline()
                                .label("打开文件夹")
                                .when_some(handler, |btn, h| {
                                    btn.on_click(move |e, w, cx| h(e, w, cx))
                                })
                        )
                    })
                    // 删除按钮（非活动任务）
                    .when(!is_active, |this| {
                        let handler = on_delete.clone();
                        this.child(
                            Button::new("delete")
                                .xsmall()
                                .danger()
                                .label("删除")
                                .when_some(handler, |btn, h| {
                                    btn.on_click(move |e, w, cx| h(e, w, cx))
                                })
                        )
                    })
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

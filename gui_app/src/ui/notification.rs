// gui_app/src/ui/notification.rs
//! 通知系统组件 - Toast通知、进度指示器、错误处理UI

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::*;
use gpui_component::button::{Button, ButtonVariants};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 通知类型
#[derive(Debug, Clone, PartialEq)]
pub enum NotificationType {
    /// 信息
    Info,
    /// 成功
    Success,
    /// 警告
    Warning,
    /// 错误
    Error,
    /// 进度
    Progress(f32),
}

impl NotificationType {
    fn icon(&self) -> &'static str {
        match self {
            NotificationType::Info => "ℹ️",
            NotificationType::Success => "✅",
            NotificationType::Warning => "⚠️",
            NotificationType::Error => "❌",
            NotificationType::Progress(_) => "⏳",
        }
    }
}

/// 通知消息
#[derive(Debug, Clone)]
pub struct Notification {
    /// 唯一ID
    pub id: String,
    /// 标题
    pub title: String,
    /// 消息内容
    pub message: Option<String>,
    /// 通知类型
    pub notification_type: NotificationType,
    /// 创建时间
    pub created_at: Instant,
    /// 持续时间（None表示不自动关闭）
    pub duration: Option<Duration>,
    /// 是否可关闭
    pub dismissible: bool,
    /// 操作按钮
    pub actions: Vec<NotificationAction>,
}

/// 通知操作
#[derive(Debug, Clone)]
pub struct NotificationAction {
    /// 按钮文字
    pub label: String,
    /// 操作ID
    pub action_id: String,
}

impl Notification {
    /// 创建信息通知
    pub fn info(title: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.into(),
            message: None,
            notification_type: NotificationType::Info,
            created_at: Instant::now(),
            duration: Some(Duration::from_secs(5)),
            dismissible: true,
            actions: Vec::new(),
        }
    }

    /// 创建成功通知
    pub fn success(title: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.into(),
            message: None,
            notification_type: NotificationType::Success,
            created_at: Instant::now(),
            duration: Some(Duration::from_secs(4)),
            dismissible: true,
            actions: Vec::new(),
        }
    }

    /// 创建警告通知
    pub fn warning(title: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.into(),
            message: None,
            notification_type: NotificationType::Warning,
            created_at: Instant::now(),
            duration: Some(Duration::from_secs(6)),
            dismissible: true,
            actions: Vec::new(),
        }
    }

    /// 创建错误通知
    pub fn error(title: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.into(),
            message: None,
            notification_type: NotificationType::Error,
            created_at: Instant::now(),
            duration: None, // 错误不自动关闭
            dismissible: true,
            actions: Vec::new(),
        }
    }

    /// 创建进度通知
    pub fn progress(title: impl Into<String>, progress: f32) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.into(),
            message: None,
            notification_type: NotificationType::Progress(progress),
            created_at: Instant::now(),
            duration: None,
            dismissible: false,
            actions: Vec::new(),
        }
    }

    /// 设置消息内容
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// 设置持续时间
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// 添加操作按钮
    pub fn with_action(mut self, label: impl Into<String>, action_id: impl Into<String>) -> Self {
        self.actions.push(NotificationAction {
            label: label.into(),
            action_id: action_id.into(),
        });
        self
    }

    /// 检查是否过期
    pub fn is_expired(&self) -> bool {
        if let Some(duration) = self.duration {
            self.created_at.elapsed() > duration
        } else {
            false
        }
    }

    /// 更新进度
    pub fn update_progress(&mut self, progress: f32) {
        self.notification_type = NotificationType::Progress(progress);
    }
}

/// 通知容器事件
#[derive(Debug, Clone)]
pub enum NotificationEvent {
    /// 关闭通知
    Dismiss(String),
    /// 执行操作
    Action(String, String),
}

/// 通知容器回调
pub type NotificationCallback = Arc<dyn Fn(NotificationEvent) + Send + Sync>;

/// 通知容器
pub struct NotificationContainer {
    /// 通知列表
    notifications: Vec<Notification>,
    /// 最大显示数量
    max_visible: usize,
    /// 事件回调
    callback: Option<NotificationCallback>,
}

impl NotificationContainer {
    /// 创建新的通知容器
    pub fn new() -> Self {
        Self {
            notifications: Vec::new(),
            max_visible: 5,
            callback: None,
        }
    }

    /// 设置回调
    pub fn on_event(mut self, callback: NotificationCallback) -> Self {
        self.callback = Some(callback);
        self
    }

    /// 添加通知
    pub fn push(&mut self, notification: Notification) {
        self.notifications.push(notification);
        // 保持最大数量限制
        while self.notifications.len() > self.max_visible {
            self.notifications.remove(0);
        }
    }

    /// 关闭通知
    pub fn dismiss(&mut self, id: &str) {
        self.notifications.retain(|n| n.id != id);
    }

    /// 清理过期通知
    pub fn cleanup_expired(&mut self) {
        self.notifications.retain(|n| !n.is_expired());
    }

    /// 更新进度通知
    pub fn update_progress(&mut self, id: &str, progress: f32) {
        if let Some(notification) = self.notifications.iter_mut().find(|n| n.id == id) {
            notification.update_progress(progress);
        }
    }

    /// 清空所有通知
    pub fn clear(&mut self) {
        self.notifications.clear();
    }
}

impl Render for NotificationContainer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bg = theme.background;
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let border = theme.border;
        let primary = theme.primary;
        let success = theme.success;
        let warning = theme.warning;
        let danger = theme.danger;
        
        // 清理过期通知
        self.cleanup_expired();

        // 渲染通知栈
        div()
            .absolute()
            .bottom(px(16.0))
            .right(px(16.0))
            .w(px(360.0))
            .child(
                v_flex()
                    .gap_2()
                    .children(self.notifications.iter().rev().take(self.max_visible).map(|notification| {
                        render_notification_toast_inline(notification, bg, fg, muted, border, primary, success, warning, danger)
                    }))
            )
    }
}

/// 渲染单个通知 Toast（内联版本，不需要 cx）
fn render_notification_toast_inline(
    notification: &Notification, 
    bg: Hsla,
    fg: Hsla,
    muted: Hsla,
    border: Hsla,
    primary: Hsla,
    success: Hsla,
    warning: Hsla,
    danger: Hsla,
) -> impl IntoElement {
    let type_color = match &notification.notification_type {
        NotificationType::Info => primary,
        NotificationType::Success => success,
        NotificationType::Warning => warning,
        NotificationType::Error => danger,
        NotificationType::Progress(_) => primary,
    };

    let id = notification.id.clone();
    let title = notification.title.clone();
    let message = notification.message.clone();
    let icon = notification.notification_type.icon();
    let dismissible = notification.dismissible;
    let actions = notification.actions.clone();

    v_flex()
        .w_full()
        .p_3()
        .rounded_lg()
        .bg(bg)
        .border_1()
        .border_color(border)
        .shadow_lg()
        // 头部
        .child(
            h_flex()
                .gap_2()
                .items_start()
                // 图标
                .child(
                    div()
                        .text_base()
                        .child(icon)
                )
                // 内容
                .child(
                    v_flex()
                        .flex_1()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(fg)
                                .child(title)
                        )
                        .when(message.is_some(), |this| {
                            this.child(
                                div()
                                    .text_sm()
                                    .text_color(muted)
                                    .child(message.unwrap())
                            )
                        })
                )
                // 关闭按钮
                .when(dismissible, |this| {
                    this.child(
                        Button::new(SharedString::from(format!("dismiss-{}", id)))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close)
                    )
                })
        )
        // 进度条（如果是进度类型）
        .when(matches!(notification.notification_type, NotificationType::Progress(_)), |this| {
            if let NotificationType::Progress(progress) = notification.notification_type {
                this.child(
                    div()
                        .mt_2()
                        .child(render_progress_bar(progress, type_color, border))
                )
            } else {
                this
            }
        })
        // 操作按钮
        .when(!actions.is_empty(), |this| {
            this.child(
                h_flex()
                    .mt_2()
                    .gap_2()
                    .justify_end()
                    .children(actions.iter().map(|action| {
                        Button::new(SharedString::from(format!("action-{}-{}", id, action.action_id)))
                            .ghost()
                            .small()
                            .label(SharedString::from(action.label.clone()))
                            .into_any_element()
                    }))
            )
        })
}

/// 渲染进度条
fn render_progress_bar(progress: f32, color: Hsla, border: Hsla) -> impl IntoElement {
    let progress_clamped = progress.clamp(0.0, 1.0);
    let percentage = (progress_clamped * 100.0) as u32;

    h_flex()
        .gap_2()
        .items_center()
        // 进度条
        .child(
            div()
                .flex_1()
                .h(px(4.0))
                .rounded_full()
                .bg(border)
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .bg(color)
                        .w(relative(progress_clamped))
                )
        )
        // 百分比
        .child(
            div()
                .text_xs()
                .w(px(36.0))
                .text_right()
                .child(format!("{}%", percentage))
        )
}

/// 进度指示器组件（全屏遮罩）
pub struct ProgressOverlay {
    /// 是否显示
    pub visible: bool,
    /// 标题
    pub title: String,
    /// 消息
    pub message: Option<String>,
    /// 进度（0.0-1.0）
    pub progress: Option<f32>,
    /// 是否可取消
    pub cancellable: bool,
}

impl ProgressOverlay {
    /// 创建新的进度遮罩
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            visible: false,
            title: title.into(),
            message: None,
            progress: None,
            cancellable: false,
        }
    }

    /// 显示
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// 隐藏
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// 设置消息
    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
    }

    /// 设置进度
    pub fn set_progress(&mut self, progress: f32) {
        self.progress = Some(progress.clamp(0.0, 1.0));
    }

    /// 设置可取消
    pub fn set_cancellable(&mut self, cancellable: bool) {
        self.cancellable = cancellable;
    }
}

impl Render for ProgressOverlay {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bg = theme.background;
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let primary = theme.primary;
        let border = theme.border;

        if !self.visible {
            return div().into_any_element();
        }

        let title = self.title.clone();
        let message = self.message.clone();
        let progress = self.progress;
        let cancellable = self.cancellable;

        div()
            .absolute()
            .inset_0()
            .bg(Hsla::from(gpui::black()).opacity(0.5))
            .flex()
            .items_center()
            .justify_center()
            .child(
                v_flex()
                    .w(px(320.0))
                    .p_6()
                    .rounded_xl()
                    .bg(bg)
                    .shadow_xl()
                    .gap_4()
                    .items_center()
                    // 加载动画
                    .child(
                        div()
                            .text_2xl()
                            .child("⏳")
                    )
                    // 标题
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg)
                            .text_center()
                            .child(title)
                    )
                    // 消息
                    .when(message.is_some(), |this| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(muted)
                                .text_center()
                                .child(message.unwrap())
                        )
                    })
                    // 进度条
                    .when(progress.is_some(), |this| {
                        this.child(
                            div()
                                .w_full()
                                .child(render_progress_bar(progress.unwrap(), primary, border))
                        )
                    })
                    // 取消按钮
                    .when(cancellable, |this| {
                        this.child(
                            Button::new("cancel-progress")
                                .ghost()
                                .label("取消")
                        )
                    })
            )
            .into_any_element()
    }
}

/// 确认对话框
pub struct ConfirmDialog {
    /// 是否显示
    pub visible: bool,
    /// 标题
    pub title: String,
    /// 消息
    pub message: String,
    /// 确认按钮文字
    pub confirm_label: String,
    /// 取消按钮文字
    pub cancel_label: String,
    /// 是否为危险操作
    pub is_danger: bool,
    /// 回调
    pub on_confirm: Option<Arc<dyn Fn() + Send + Sync>>,
    pub on_cancel: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl ConfirmDialog {
    /// 创建新的确认对话框
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            visible: false,
            title: title.into(),
            message: message.into(),
            confirm_label: "确定".to_string(),
            cancel_label: "取消".to_string(),
            is_danger: false,
            on_confirm: None,
            on_cancel: None,
        }
    }

    /// 创建危险操作确认
    pub fn danger(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            visible: false,
            title: title.into(),
            message: message.into(),
            confirm_label: "确定删除".to_string(),
            cancel_label: "取消".to_string(),
            is_danger: true,
            on_confirm: None,
            on_cancel: None,
        }
    }

    /// 显示
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// 隐藏
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// 设置确认按钮文字
    pub fn with_confirm_label(mut self, label: impl Into<String>) -> Self {
        self.confirm_label = label.into();
        self
    }

    /// 设置取消按钮文字
    pub fn with_cancel_label(mut self, label: impl Into<String>) -> Self {
        self.cancel_label = label.into();
        self
    }
}

impl Render for ConfirmDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bg = theme.background;
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let border = theme.border;

        if !self.visible {
            return div().into_any_element();
        }

        let title = self.title.clone();
        let message = self.message.clone();
        let confirm_label = self.confirm_label.clone();
        let cancel_label = self.cancel_label.clone();
        let is_danger = self.is_danger;

        div()
            .absolute()
            .inset_0()
            .bg(Hsla::from(gpui::black()).opacity(0.5))
            .flex()
            .items_center()
            .justify_center()
            .child(
                v_flex()
                    .w(px(400.0))
                    .p_6()
                    .rounded_xl()
                    .bg(bg)
                    .shadow_xl()
                    .gap_4()
                    // 图标
                    .child(
                        div()
                            .text_2xl()
                            .text_center()
                            .child(if is_danger { "⚠️" } else { "❓" })
                    )
                    // 标题
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg)
                            .text_center()
                            .child(title)
                    )
                    // 消息
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .text_center()
                            .child(message)
                    )
                    // 按钮
                    .child(
                        h_flex()
                            .mt_2()
                            .gap_3()
                            .justify_center()
                            .child(
                                Button::new("dialog-cancel")
                                    .outline()
                                    .label(SharedString::from(cancel_label))
                            )
                            .child(
                                Button::new("dialog-confirm")
                                    .when(is_danger, |btn| btn.danger())
                                    .when(!is_danger, |btn| btn.primary())
                                    .label(SharedString::from(confirm_label))
                            )
                    )
            )
            .into_any_element()
    }
}

/// 错误详情面板
pub struct ErrorPanel {
    /// 是否显示
    pub visible: bool,
    /// 错误标题
    pub title: String,
    /// 错误消息
    pub message: String,
    /// 详细信息
    pub details: Option<String>,
    /// 重试回调
    pub on_retry: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl ErrorPanel {
    /// 创建错误面板
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            visible: false,
            title: title.into(),
            message: message.into(),
            details: None,
            on_retry: None,
        }
    }

    /// 显示
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// 隐藏
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// 设置详细信息
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

impl Render for ErrorPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bg = theme.background;
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let border = theme.border;

        if !self.visible {
            return div().into_any_element();
        }

        let title = self.title.clone();
        let message = self.message.clone();
        let details = self.details.clone();
        let has_retry = self.on_retry.is_some();

        v_flex()
            .w_full()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(danger.opacity(0.3))
            .bg(danger.opacity(0.05))
            .gap_3()
            // 头部
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Icon::new(IconName::TriangleAlert)
                            .size(px(20.0))
                            .text_color(danger)
                    )
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(danger)
                            .child(title)
                    )
            )
            // 消息
            .child(
                div()
                    .text_sm()
                    .text_color(fg)
                    .child(message)
            )
            // 详细信息
            .when(details.is_some(), |this| {
                this.child(
                    div()
                        .p_3()
                        .rounded_md()
                        .bg(bg)
                        .border_1()
                        .border_color(border)
                        .max_h(px(150.0))
                        .overflow_hidden()
                        .child(
                            div()
                                .text_xs()
                                .font_family("monospace")
                                .text_color(muted)
                                .child(details.unwrap())
                        )
                )
            })
            // 操作按钮
            .child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .when(has_retry, |this| {
                        this.child(
                            Button::new("error-retry")
                                .outline()
                                .small()
                                .label("重试")
                        )
                    })
                    .child(
                        Button::new("error-close")
                            .ghost()
                            .small()
                            .label("关闭")
                    )
            )
            .into_any_element()
    }
}

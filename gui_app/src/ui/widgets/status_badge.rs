//! 状态徽章组件
//!
//! 显示各种状态的小标签

use gpui::prelude::FluentBuilder;
use gpui::*;

/// 徽章类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeVariant {
    /// 成功/已安装
    Success,
    /// 警告/需要更新
    Warning,
    /// 错误/未安装
    Error,
    /// 信息/默认
    Info,
    /// 进行中
    Progress,
}

impl BadgeVariant {
    fn colors(&self) -> (Rgba, Rgba, Rgba) {
        match self {
            Self::Success => (rgb(0x052e16), rgb(0x166534), rgb(0x4ade80)),
            Self::Warning => (rgb(0x422006), rgb(0x854d0e), rgb(0xfbbf24)),
            Self::Error => (rgb(0x450a0a), rgb(0x991b1b), rgb(0xfca5a5)),
            Self::Info => (rgb(0x172554), rgb(0x1e40af), rgb(0x60a5fa)),
            Self::Progress => (rgb(0x1e1b4b), rgb(0x4338ca), rgb(0xa78bfa)),
        }
    }
}

/// 状态徽章组件
#[derive(IntoElement)]
pub struct StatusBadge {
    label: String,
    variant: BadgeVariant,
    icon: Option<String>,
}

impl StatusBadge {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            variant: BadgeVariant::Info,
            icon: None,
        }
    }

    pub fn variant(mut self, variant: BadgeVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn success(mut self) -> Self {
        self.variant = BadgeVariant::Success;
        self
    }

    pub fn warning(mut self) -> Self {
        self.variant = BadgeVariant::Warning;
        self
    }

    pub fn error(mut self) -> Self {
        self.variant = BadgeVariant::Error;
        self
    }

    pub fn progress(mut self) -> Self {
        self.variant = BadgeVariant::Progress;
        self
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

impl RenderOnce for StatusBadge {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let (bg, border, text) = self.variant.colors();

        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(px(8.0))
            .py(px(4.0))
            .bg(bg)
            .border_1()
            .border_color(border)
            .rounded(px(6.0))
            .when_some(self.icon, |el, icon| el.child(div().text_xs().child(icon)))
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(text)
                    .child(self.label),
            )
    }
}

use crate::app::{CaptureResourceType, M3u8Stream};
use gpui::prelude::FluentBuilder;
use gpui::*;

#[derive(Clone, Copy)]
pub struct CaptureRowTheme {
    pub bg: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
}

/// 从 URL 提取文件名作为显示标题
fn extract_display_title(url: &str) -> String {
    use url::Url;

    if let Ok(parsed_url) = Url::parse(url) {
        // 获取路径的最后一部分
        if let Some(segments) = parsed_url.path_segments() {
            if let Some(last_segment) = segments.last() {
                if !last_segment.is_empty() {
                    // URL 解码
                    if let Ok(decoded) = urlencoding::decode(last_segment) {
                        let decoded_str = decoded.to_string();
                        // 限制长度为 60 字符
                        if decoded_str.len() > 60 {
                            return format!("{}...", &decoded_str[..57]);
                        }
                        return decoded_str;
                    }

                    // 限制长度
                    if last_segment.len() > 60 {
                        return format!("{}...", &last_segment[..57]);
                    }
                    return last_segment.to_string();
                }
            }
        }

        // 如果路径为空，使用域名
        if let Some(host) = parsed_url.host_str() {
            return host.to_string();
        }
    }

    // 降级方案：截断 URL
    if url.len() > 60 {
        format!("{}...", &url[..57])
    } else {
        url.to_string()
    }
}

pub fn capture_row(
    item: M3u8Stream,
    status: Option<String>,
    theme: CaptureRowTheme,
    action: impl IntoElement,
) -> impl IntoElement {
    // 使用 URL 文件名作为标题（优先），而不是 pageTitle
    let display_title = extract_display_title(&item.url);

    // 截断 URL 显示（用于鼠标悬停或详情）
    let url_display = if item.url.len() > 80 {
        format!("{}...", &item.url[..77])
    } else {
        item.url.clone()
    };

    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .p(px(12.0))
        .bg(theme.bg)
        .border_1()
        .border_color(theme.border)
        .rounded(px(10.0))
        .hover(|this| this.bg(theme.border.opacity(0.05)))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    // 资源类型徽章
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(px(6.0))
                        .bg(match item.resource_type {
                            CaptureResourceType::Video => rgb(0x3b82f6).into(),
                            CaptureResourceType::Audio => rgb(0x10b981).into(),
                            CaptureResourceType::Image => rgb(0x8b5cf6).into(),
                            _ => theme.border,
                        }
                        .opacity(0.15))
                        .text_color(match item.resource_type {
                            CaptureResourceType::Video => rgb(0x3b82f6).into(),
                            CaptureResourceType::Audio => rgb(0x10b981).into(),
                            CaptureResourceType::Image => rgb(0x8b5cf6).into(),
                            _ => theme.text,
                        })
                        .child(match item.resource_type {
                            CaptureResourceType::Video => "视频",
                            CaptureResourceType::Audio => "音频",
                            CaptureResourceType::Image => "图片",
                            CaptureResourceType::Document => "文档",
                            CaptureResourceType::Font => "字体",
                            CaptureResourceType::Stylesheet => "样式",
                            CaptureResourceType::Script => "脚本",
                            CaptureResourceType::Other => "其他",
                        }),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child(display_title),
                )
                .when_some(item.duration_seconds, |row, secs| {
                    row.child(
                        div()
                            .text_xs()
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded(px(6.0))
                            .bg(theme.border.opacity(0.15))
                            .text_color(theme.muted)
                            .child(format!("⏱ {}", format_secs(secs))),
                    )
                }),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted)
                .line_height(relative(1.4))
                .child(url_display),
        )
        .child(
            div()
                .flex()
                .gap(px(10.0))
                .items_center()
                .child(action)
                .when_some(status, |row, s| {
                    row.child(
                        div()
                            .text_xs()
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded(px(6.0))
                            .bg(theme.border.opacity(0.1))
                            .text_color(theme.text)
                            .child(s),
                    )
                }),
        )
}

fn format_secs(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{:02}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

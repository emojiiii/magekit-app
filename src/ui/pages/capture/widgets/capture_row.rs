use crate::app::{CaptureResourceType, M3u8Stream};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_kit::component::Sizable;
use gpui_kit::component::tag::Tag;
use magekit_shared::truncate_string;

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
                        return truncate_string(&decoded_str, 60);
                    }

                    // 限制长度
                    return truncate_string(last_segment, 60);
                }
            }
        }

        // 如果路径为空，使用域名
        if let Some(host) = parsed_url.host_str() {
            return host.to_string();
        }
    }

    // 降级方案：截断 URL
    truncate_string(url, 60)
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
    let url_display = truncate_string(&item.url, 80);

    div()
        .w_full()
        .min_w_0()
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
                .child(Tag::secondary().small().child(match item.resource_type {
                    CaptureResourceType::Video => crate::i18n::tr("视频"),
                    CaptureResourceType::Audio => crate::i18n::tr("音频"),
                    CaptureResourceType::Image => crate::i18n::tr("图片"),
                    CaptureResourceType::Document => crate::i18n::tr("文档"),
                    CaptureResourceType::Font => crate::i18n::tr("字体"),
                    CaptureResourceType::Stylesheet => crate::i18n::tr("样式"),
                    CaptureResourceType::Script => crate::i18n::tr("脚本"),
                    CaptureResourceType::Other => crate::i18n::tr("其他"),
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child(display_title),
                )
                .when_some(item.duration_seconds, |row, secs| {
                    row.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded(px(6.0))
                            .bg(theme.border.opacity(0.15))
                            .text_color(theme.muted)
                            .child(format_secs(secs)),
                    )
                }),
        )
        .child(
            div()
                .text_xs()
                .truncate()
                .text_color(theme.muted)
                .line_height(relative(1.4))
                .child(url_display),
        )
        .child(
            div()
                .flex()
                .gap(px(10.0))
                .items_center()
                .child(div().flex_shrink_0().child(action))
                .when_some(status, |row, s| {
                    row.child(
                        div()
                            .text_xs()
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded(px(6.0))
                            .bg(theme.border.opacity(0.1))
                            .text_color(theme.text)
                            .child(crate::i18n::text(&s)),
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

#[cfg(test)]
mod tests {
    use super::extract_display_title;

    #[test]
    fn encoded_unicode_media_names_do_not_split_utf8() {
        // An ASCII prefix makes the old byte-57 slice land inside a Chinese character.
        let name = format!("a{}.mp4", "视频".repeat(40));
        let url = format!("https://example.com/{}", urlencoding::encode(&name));
        let title = extract_display_title(&url);
        assert!(title.starts_with("a视频"));
        assert!(title.ends_with('…'));
        assert_eq!(title.chars().count(), 60);
    }
}

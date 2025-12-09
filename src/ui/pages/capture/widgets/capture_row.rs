use crate::app::M3u8Stream;
use gpui::prelude::FluentBuilder;
use gpui::*;

#[derive(Clone, Copy)]
pub struct CaptureRowTheme {
    pub bg: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
}

pub fn capture_row(
    item: M3u8Stream,
    status: Option<String>,
    output_dir: String,
    theme: CaptureRowTheme,
    action: impl IntoElement,
) -> impl IntoElement {
    // 为长链接分隔插入零宽空格
    let label = item
        .title
        .clone()
        .unwrap_or_else(|| {
            item.url
                .replace('/', "/\u{200b}")
                .replace('?', "?\u{200b}")
                .replace('&', "&\u{200b}")
        });

    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .p(px(10.0))
        .bg(theme.bg)
        .border_1()
        .border_color(theme.border)
        .rounded(px(8.0))
        .child(div().text_sm().text_color(theme.text).child(label))
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .items_center()
                .child(action)
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted)
                        .child(format!("输出目录: {}", output_dir)),
                )
                .when_some(status, |row, s| {
                    row.child(div().text_xs().text_color(theme.text).child(s))
                }),
        )
}


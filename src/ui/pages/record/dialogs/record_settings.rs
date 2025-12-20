//! 录制设置对话框（自绘遮罩层）

use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::Sizable;
use gpui_component::WindowExt;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};
use gpui_component::notification::Notification;
use gpui_component::radio::RadioGroup;
use gpui_component::scroll::ScrollableElement;
use gpui_component::switch::Switch;
use magekit_shared::types::{LiveRecordConfig, LiveRecordQuality};
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

/// 录制设置对话框组件
///
/// 说明：
/// - 当前 gpui-component 版本未提供统一 Modal 组件，这里复用“遮罩层 + 卡片”方案，确保有遮罩层。
/// - 配置回写通过回调交给页面（SRP）。
pub struct RecordSettingsDialog {
    selected_format: Arc<RwLock<usize>>,
    selected_quality: Arc<RwLock<usize>>,
    auto_transcode: Arc<RwLock<bool>>,
    global_auto_record: Arc<RwLock<bool>>,

    check_interval_input: Entity<InputState>,
    segment_input: Entity<InputState>,
    retry_input: Entity<InputState>,
    reconnect_input: Entity<InputState>,
    output_path_input: Entity<InputState>,

    base_config: LiveRecordConfig,
    on_save: Option<Arc<dyn Fn(LiveRecordConfig, &mut Window, &mut Context<Self>) + Send + Sync>>,
    on_cancel: Option<Arc<dyn Fn(&mut Window, &mut Context<Self>) + Send + Sync>>,
}

impl RecordSettingsDialog {
    pub fn new(base: LiveRecordConfig, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let format_options = ["ts", "mkv", "flv", "mp4"];
        let format_index = format_options
            .iter()
            .position(|f| f.eq_ignore_ascii_case(&base.record_format))
            .unwrap_or(0);

        let quality_options = [
            LiveRecordQuality::Original,
            LiveRecordQuality::Blue,
            LiveRecordQuality::Ultra,
            LiveRecordQuality::High,
            LiveRecordQuality::Standard,
        ];
        let quality_index = quality_options
            .iter()
            .position(|q| q == &base.quality)
            .unwrap_or(0);

        let check_interval_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("60")
                .default_value(base.check_interval.to_string())
        });
        let segment_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("留空表示不分段")
                .default_value(
                    base.segment_duration
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                )
        });
        let retry_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("3")
                .default_value(base.retry_count.to_string())
        });
        let reconnect_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("10")
                .default_value(base.reconnect_delay.to_string())
        });
        let output_path_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("默认：record")
                .default_value(base.output_base_path.to_string_lossy().to_string())
        });

        Self {
            selected_format: Arc::new(RwLock::new(format_index)),
            selected_quality: Arc::new(RwLock::new(quality_index)),
            auto_transcode: Arc::new(RwLock::new(base.auto_transcode)),
            global_auto_record: Arc::new(RwLock::new(base.auto_record)),
            check_interval_input,
            segment_input,
            retry_input,
            reconnect_input,
            output_path_input,
            base_config: base,
            on_save: None,
            on_cancel: None,
        }
    }

    pub fn on_save<F>(mut self, callback: F) -> Self
    where
        F: Fn(LiveRecordConfig, &mut Window, &mut Context<Self>) + Send + Sync + 'static,
    {
        self.on_save = Some(Arc::new(callback));
        self
    }

    pub fn on_cancel<F>(mut self, callback: F) -> Self
    where
        F: Fn(&mut Window, &mut Context<Self>) + Send + Sync + 'static,
    {
        self.on_cancel = Some(Arc::new(callback));
        self
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ref cb) = self.on_cancel {
            cb(window, cx);
        }
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let format_options = ["ts", "mkv", "flv", "mp4"];
        let quality_options = [
            LiveRecordQuality::Original,
            LiveRecordQuality::Blue,
            LiveRecordQuality::Ultra,
            LiveRecordQuality::High,
            LiveRecordQuality::Standard,
        ];

        let format_idx = *self.selected_format.read();
        let quality_idx = *self.selected_quality.read();

        let format = format_options.get(format_idx).unwrap_or(&"ts").to_string();
        let quality = quality_options
            .get(quality_idx)
            .cloned()
            .unwrap_or(LiveRecordQuality::Original);

        let check_interval = self
            .check_interval_input
            .read(cx)
            .text()
            .to_string()
            .trim()
            .parse::<u64>()
            .unwrap_or(60)
            .max(10);

        let segment = self
            .segment_input
            .read(cx)
            .text()
            .to_string()
            .trim()
            .parse::<u64>()
            .ok();

        let retry = self
            .retry_input
            .read(cx)
            .text()
            .to_string()
            .trim()
            .parse::<u32>()
            .unwrap_or(3)
            .max(1);

        let reconnect_delay = self
            .reconnect_input
            .read(cx)
            .text()
            .to_string()
            .trim()
            .parse::<u64>()
            .unwrap_or(10)
            .max(5);

        let output_base_path = self
            .output_path_input
            .read(cx)
            .text()
            .to_string()
            .trim()
            .to_string();

        let mut cfg = self.base_config.clone();
        cfg.record_format = format;
        cfg.quality = quality;
        cfg.auto_transcode = *self.auto_transcode.read();
        cfg.auto_record = *self.global_auto_record.read();
        cfg.check_interval = check_interval;
        cfg.segment_duration = segment;
        cfg.retry_count = retry;
        cfg.reconnect_delay = reconnect_delay;
        cfg.output_base_path = PathBuf::from(if output_base_path.is_empty() {
            "record".to_string()
        } else {
            output_base_path
        });

        if let Some(ref cb) = self.on_save {
            cb(cfg, window, cx);
        } else {
            window.push_notification(Notification::success("设置已保存"), cx);
        }
    }
}

impl Render for RecordSettingsDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bg = theme.secondary;
        let border = theme.border;
        let fg = theme.foreground;
        let muted = theme.muted_foreground;

        let selected_format = self.selected_format.clone();
        let selected_quality = self.selected_quality.clone();
        let auto_transcode = self.auto_transcode.clone();
        let global_auto_record = self.global_auto_record.clone();

        div()
            .id("record-settings-overlay")
            .absolute()
            .inset_0()
            .bg(gpui::rgba(0x00000088))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(720.0))
                    .max_w(px(880.0))
                    .max_h(px(720.0))
                    .overflow_y_scrollbar()
                    .bg(bg)
                    .border_1()
                    .border_color(border)
                    .rounded_lg()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_lg().font_weight(FontWeight::BOLD).text_color(fg).child("录制设置"))
                            .child(
                                Button::new("record-settings-close")
                                    .label("关闭")
                                    .xsmall()
                                    .ghost()
                                    .on_click(cx.listener(|this, _event, window, cx| {
                                        this.cancel(window, cx);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(fg).child("录制格式"))
                                    .child({
                                        let current_idx = *selected_format.read();
                                        RadioGroup::horizontal("record-format-radio")
                                            .children(["TS", "MKV", "FLV", "MP4"])
                                            .selected_index(Some(current_idx))
                                            .on_click({
                                                let selected_format = selected_format.clone();
                                                move |idx: &usize, _window, _cx| {
                                                    *selected_format.write() = *idx;
                                                }
                                            })
                                    })
                                    .child(div().text_xs().text_color(muted).child("TS 最稳定，MP4 兼容性最好")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(fg).child("视频质量"))
                                    .child({
                                        let current_idx = *selected_quality.read();
                                        RadioGroup::horizontal("record-quality-radio")
                                            .children(["原画", "蓝光", "超清", "高清", "标清"])
                                            .selected_index(Some(current_idx))
                                            .on_click({
                                                let selected_quality = selected_quality.clone();
                                                move |idx: &usize, _window, _cx| {
                                                    *selected_quality.write() = *idx;
                                                }
                                            })
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(fg).child("录制后自动转码"))
                                            .child(div().text_xs().text_color(muted).child("录制完成后自动转为 MP4 格式")),
                                    )
                                    .child({
                                        let checked = *auto_transcode.read();
                                        Switch::new("record-auto-transcode")
                                            .checked(checked)
                                            .on_click({
                                                let auto_transcode = auto_transcode.clone();
                                                move |checked: &bool, _window, _cx| {
                                                    *auto_transcode.write() = *checked;
                                                }
                                            })
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(fg).child("全局自动录制"))
                                            .child(div().text_xs().text_color(muted).child("仅影响“检测到开播后自动开始录制”")),
                                    )
                                    .child({
                                        let checked = *global_auto_record.read();
                                        Switch::new("record-global-auto-record")
                                            .checked(checked)
                                            .on_click({
                                                let global_auto_record = global_auto_record.clone();
                                                move |checked: &bool, _window, _cx| {
                                                    *global_auto_record.write() = *checked;
                                                }
                                            })
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(fg).child("检测间隔（秒）"))
                                    .child(Input::new(&self.check_interval_input))
                                    .child(div().text_xs().text_color(muted).child("建议 ≥ 10 秒")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(fg).child("分段时长（秒，可选）"))
                                    .child(Input::new(&self.segment_input))
                                    .child(div().text_xs().text_color(muted).child("用于分段存储/合并（实现依赖录制策略）")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(fg).child("重试次数"))
                                    .child(Input::new(&self.retry_input))
                                    .child(div().text_xs().text_color(muted).child("断流/失败后重试次数")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(fg).child("重连延迟（秒）"))
                                    .child(Input::new(&self.reconnect_input))
                                    .child(div().text_xs().text_color(muted).child("断流后等待多久重试")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(fg).child("录制输出子目录"))
                                    .child(Input::new(&self.output_path_input))
                                    .child(div().text_xs().text_color(muted).child("相对于下载目录；最终路径：{下载目录}/{子目录}/{平台}/{主播}/...")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .pt_2()
                            .border_t_1()
                            .border_color(border)
                            .child(
                                Button::new("record-settings-cancel")
                                    .label("取消")
                                    .ghost()
                                    .on_click(cx.listener(|this, _event, window, cx| {
                                        this.cancel(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("record-settings-save")
                                    .primary()
                                    .label("保存")
                                    .on_click(cx.listener(|this, _event, window, cx| {
                                        this.save(window, cx);
                                    })),
                            ),
                    ),
            )
    }
}

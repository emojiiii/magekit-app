//! 应用级中英文本地化。切换语言只刷新界面，不重建页面或后台任务。
mod core;
pub use core::{Language, Locale, Message, format, text, tr};

use gpui::{App, Context, Global, Window};
use gpui_kit::component::input::InputState;

/// GPUI 全局变更事件用于刷新缓存页面和输入提示。
#[derive(Clone, Copy)]
pub struct LocaleChanged;
impl Global for LocaleChanged {}

/// 在创建任何页面之前调用，以系统语言初始化新安装。
pub fn initialize(selection: &str) -> Locale {
    let system_locale = sys_locale::get_locale();
    let locale = Language::from_config(selection).resolve(system_locale.as_deref());
    core::set_locale(locale);
    gpui_kit::component::set_locale(locale.code());
    locale
}

pub fn apply_language(selection: Language, cx: &mut App) {
    initialize(selection.config_value());
    cx.set_global(LocaleChanged);
    cx.refresh_windows();
}

/// 为已有 InputState 安装语言订阅；保留输入文本、光标、选区和撤销历史。
pub fn input(
    placeholder: &'static str,
    window: &mut Window,
    cx: &mut Context<InputState>,
) -> InputState {
    cx.observe_global_in::<LocaleChanged>(window, move |input, window, cx| {
        input.set_placeholder(tr(placeholder), window, cx);
    })
    .detach();
    InputState::new(window, cx).placeholder(tr(placeholder))
}

//! MageKit 对 `proc-macro-error2` 的最小兼容实现。
//!
//! `gpui 0.2.2` 通过 `stacksafe-macro` 间接依赖该 crate，但 crates.io 上的
//! `2.0.1` 会触发 Rust 的未来不兼容检查（重新导出私有 `proc_macro`）。
//! 当前依赖只使用属性宏、`abort!`、`abort_call_site!` 和 `entry_point`，因此
//! 仅实现这组 API，避免把未修复的上游实现带入应用构建。

#![forbid(unsafe_code)]

extern crate proc_macro;

pub use proc_macro_error_attr2::proc_macro_error;

use std::panic::{catch_unwind, UnwindSafe};

/// 执行过程宏主体，并将未处理的 panic 转换为编译错误。
pub fn entry_point<F>(f: F, _proc_macro_hack: bool) -> proc_macro::TokenStream
where
    F: FnOnce() -> proc_macro::TokenStream + UnwindSafe,
{
    match catch_unwind(f) {
        Ok(tokens) => tokens,
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("procedural macro panicked");
            compile_error(message)
        }
    }
}

#[doc(hidden)]
pub fn compile_error(message: impl AsRef<str>) -> proc_macro::TokenStream {
    let message = proc_macro2::Literal::string(message.as_ref());
    quote::quote! { compile_error!(#message); }.into()
}

/// 生成带有调用位置上下文的编译错误并立即结束过程宏。
#[macro_export]
macro_rules! abort {
    ($span:expr, $message:literal $(, $args:expr)* $(,)?) => {{
        let _ = &$span;
        return $crate::compile_error(format!($message $(, $args)*));
    }};
}

/// 生成调用位置编译错误并立即结束过程宏。
#[macro_export]
macro_rules! abort_call_site {
    ($message:literal $(, $args:expr)* $(,)?) => {{
        return $crate::compile_error(format!($message $(, $args)*));
    }};
}

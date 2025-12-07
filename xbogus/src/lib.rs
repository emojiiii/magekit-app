//! X-Bogus 签名生成库
//!
//! 使用 QuickJS 运行 JavaScript 代码来生成 X-Bogus 签名
//! 
//! 同时提供 AB-Sign 的 Rust 原生实现

use rquickjs::{CatchResultExt, Context, Function, Runtime};
use std::fmt;

pub mod ab_sign;

// 导出 AB-Sign
pub use ab_sign::ab_sign;

// 导出内部模块用于测试
pub use ab_sign::sm3;
pub use ab_sign::rc4;
pub use ab_sign::base64_custom;

/// JavaScript 源代码
const JS_SOURCE: &str = include_str!("x-bogus.js");

/// X-Bogus 错误类型
#[derive(Debug)]
pub enum XBogusError {
    /// 运行时创建失败
    RuntimeCreation(String),
    /// 上下文创建失败
    ContextCreation(String),
    /// JavaScript 执行失败
    JsExecution(String),
    /// 签名函数调用失败
    SignFunction(String),
}

impl fmt::Display for XBogusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            XBogusError::RuntimeCreation(msg) => write!(f, "Runtime creation failed: {}", msg),
            XBogusError::ContextCreation(msg) => write!(f, "Context creation failed: {}", msg),
            XBogusError::JsExecution(msg) => write!(f, "JS execution failed: {}", msg),
            XBogusError::SignFunction(msg) => write!(f, "Sign function failed: {}", msg),
        }
    }
}

impl std::error::Error for XBogusError {}

/// X-Bogus 签名生成器
pub struct XBogus {
    #[allow(dead_code)]
    runtime: Runtime,
    context: Context,
}

impl XBogus {
    /// 创建新的 X-Bogus 签名生成器
    pub fn new() -> Result<Self, XBogusError> {
        let runtime = Runtime::new()
            .map_err(|e| XBogusError::RuntimeCreation(e.to_string()))?;
        let context = Context::full(&runtime)
            .map_err(|e| XBogusError::ContextCreation(e.to_string()))?;

        // 初始化 JavaScript 环境
        context.with(|ctx| {
            // 执行 JS 代码来定义 sign 函数
            let result = ctx.eval::<(), _>(JS_SOURCE).catch(&ctx);
            match result {
                Ok(_) => Ok(()),
                Err(e) => Err(XBogusError::JsExecution(format!("{:?}", e))),
            }
        })?;

        Ok(Self { runtime, context })
    }

    /// 生成 X-Bogus 签名
    ///
    /// # Arguments
    /// * `query` - 查询字符串（URL 参数）
    /// * `user_agent` - User-Agent 字符串
    ///
    /// # Returns
    /// 返回生成的 X-Bogus 签名字符串
    pub fn sign(&self, query: &str, user_agent: &str) -> Result<String, XBogusError> {
        println!("🔐 XBogus::sign called");
        println!("  query: {}", query);
        println!("  user_agent: {}", user_agent);
        
        self.context.with(|ctx| {
            // 获取 sign 函数
            let sign_fn_result: Result<Function, _> = ctx.globals().get("sign").catch(&ctx);
            let sign_fn = sign_fn_result
                .map_err(|e| XBogusError::SignFunction(format!("Failed to get sign function: {:?}", e)))?;

            // 调用 sign 函数
            let result: Result<String, _> = sign_fn.call((query, user_agent)).catch(&ctx);
            let signature = result.map_err(|e| XBogusError::SignFunction(format!("Failed to call sign function: {:?}", e)))?;
            
            println!("  result: {}", signature);
            Ok(signature)
        })
    }
}

impl Default for XBogus {
    fn default() -> Self {
        Self::new().expect("Failed to create XBogus instance")
    }
}

/// 便捷函数：直接生成 X-Bogus 签名
///
/// # Arguments
/// * `query` - 查询字符串（URL 参数）
/// * `user_agent` - User-Agent 字符串
///
/// # Returns
/// 返回生成的 X-Bogus 签名字符串
pub fn sign(query: &str, user_agent: &str) -> Result<String, XBogusError> {
    let xbogus = XBogus::new()?;
    xbogus.sign(query, user_agent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xbogus_creation() {
        let xbogus = XBogus::new();
        assert!(xbogus.is_ok(), "Failed to create XBogus: {:?}", xbogus.err());
    }

    #[test]
    fn test_sign_basic() {
        let xbogus = XBogus::new().expect("Failed to create XBogus");
        let query = "device_platform=webapp&aid=6383&channel=channel_pc_web";
        let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

        let result = xbogus.sign(query, user_agent);
        println!("Sign result: {:?}", result);
        assert!(result.is_ok(), "Sign failed: {:?}", result.err());

        let signature = result.unwrap();
        println!("Signature: {}", signature);
        assert!(!signature.is_empty(), "Signature should not be empty");
    }

    #[test]
    fn test_sign_convenience_function() {
        let query = "device_platform=webapp&aid=6383&channel=channel_pc_web";
        let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

        let result = sign(query, user_agent);
        println!("Sign result: {:?}", result);
        assert!(result.is_ok(), "Sign failed: {:?}", result.err());
    }
}


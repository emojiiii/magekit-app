//! 签名算法模块
//!
//! 包含字节跳动平台所需的签名算法实现

pub mod abogus;
mod base64_custom;
mod rc4;
pub mod sm3;
mod xbogus;

// 重新导出签名函数
pub use abogus::generate_abogus as ab_sign;
pub use abogus::ab_sign_live;
pub use xbogus::generate_xbogus as xbogus_sign;

// 导出内部模块用于高级用法
pub use abogus::AbogusOptions;
pub use base64_custom::result_encrypt;
pub use rc4::rc4_encrypt;

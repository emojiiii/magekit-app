//! AB-Sign 签名生成库
//!
//! 移植自 DouyinLiveRecorder 的 Python 实现

pub mod base64_custom;
pub mod rc4;
mod sign;
pub mod sm3;

pub use sign::ab_sign;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ab_sign() {
        let query = "aid=6383&app_name=douyin_web&live_id=1&device_platform=web&language=zh-CN&browser_language=zh-CN&browser_platform=Win32&browser_name=Chrome&browser_version=116.0.0.0&web_rid=313899056971&msToken=";
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

        let result = ab_sign(query, ua);
        println!("AB-Sign result: {}", result);

        // Python 生成的结果：E7mhBmg6mEVNgf6X51/LfY3q6lB3Y68s0HViMD2fdVfoBy39HMYd9exoRdhvGC6jiT/QIeYjy4hbO3xprQAjM36UHWwEUdQ2mgWkKl5Q5I0j53iruyRDntmF4vj3SFlm5XNAEOk=
        assert!(!result.is_empty());
        assert!(result.ends_with('='));
    }
}

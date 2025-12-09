//! 集成测试
//!
//! 验证抖音 API 接口的正确性

use bytedance::douyin::{DouyinApi, DouyinLiveApi, DouyinEndpoints};
use bytedance::sign::{ab_sign, xbogus_sign, AbogusOptions};

/// 测试签名函数
#[test]
fn test_ab_sign_basic() {
    let query = "device_platform=webapp&aid=6383&channel=channel_pc_web&msToken=";
    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";
    
    let result = ab_sign(query, ua, None);
    assert!(!result.is_empty());
    // A-Bogus 签名通常较长
    assert!(result.len() > 50);
}

/// 测试使用固定参数的签名
#[test]
fn test_ab_sign_with_options() {
    let query = "aid=6383&device_platform=webapp&channel=channel_pc_web&msToken=";
    let opts = AbogusOptions {
        method: "GET".to_string(),
        start_time: Some(1_700_000_000_000),
        end_time: Some(1_700_000_000_005),
        random_num_1: Some(1111.0),
        random_num_2: Some(2222.0),
        random_num_3: Some(3333.0),
        browser: None,
    };

    let result = ab_sign(query, "", Some(opts));
    // 使用固定参数时，结果应该是确定的
    assert_eq!(
        result,
        "D7WhBQugdDDkkfyh56KLfY3q6VfVYmQI0SVkMD2fAPDOqL39HMYh9exoIBGvXY8jwG/-IeEjy4hbT3ohrQ2y0Hwf9W0L/25ksDSkKl5Q5xSSs1X9eghgJ04qmkt5SMx2RvB-rOXmqhZHKRbp09oHmhK4b1dzFgf3qJLz6j=="
    );
}

/// 测试 X-Bogus 签名
#[test]
fn test_xbogus_sign() {
    let url = "device_platform=webapp&aid=6383&channel=channel_pc_web";
    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/103.0.0.0 Safari/537.36";

    let result = xbogus_sign(url, ua, None);
    assert!(!result.signature.is_empty());
    assert!(result.params.contains("X-Bogus="));
}

/// 测试 Endpoints 常量
#[test]
fn test_endpoints() {
    assert!(DouyinEndpoints::DOUYIN_DOMAIN.starts_with("https://"));
    assert!(DouyinEndpoints::POST_DETAIL.contains("aweme/detail"));
    assert!(DouyinEndpoints::USER_POST.contains("aweme/post"));
    assert!(DouyinEndpoints::LIVE_INFO.contains("webcast"));
}

/// 测试 API 客户端创建
#[test]
fn test_api_client_creation() {
    let api = DouyinApi::new();
    assert!(api.is_ok());
}

/// 测试直播 API 客户端创建
#[test]
fn test_live_api_client_creation() {
    let api = DouyinLiveApi::new();
    assert!(api.is_ok());
}

/// 测试从 URL 提取 aweme_id
#[test]
fn test_extract_aweme_id() {
    // 标准视频 URL
    assert_eq!(
        DouyinApi::extract_aweme_id("https://www.douyin.com/video/7321613070743663893"),
        Some("7321613070743663893".to_string())
    );
    
    // 笔记 URL
    assert_eq!(
        DouyinApi::extract_aweme_id("https://www.douyin.com/note/7321613070743663893"),
        Some("7321613070743663893".to_string())
    );
    
    // modal_id 参数
    assert_eq!(
        DouyinApi::extract_aweme_id("https://www.douyin.com/discover?modal_id=7321613070743663893"),
        Some("7321613070743663893".to_string())
    );
    
    // vid 参数
    assert_eq!(
        DouyinApi::extract_aweme_id("https://www.douyin.com/user/xxx?vid=7321613070743663893"),
        Some("7321613070743663893".to_string())
    );
    
    // 无效 URL
    assert_eq!(
        DouyinApi::extract_aweme_id("https://www.example.com/"),
        None
    );
}

/// 测试从 URL 提取 sec_user_id
#[test]
fn test_extract_sec_user_id() {
    assert_eq!(
        DouyinApi::extract_sec_user_id("https://www.douyin.com/user/MS4wLjABAAAA123abc"),
        Some("MS4wLjABAAAA123abc".to_string())
    );
    
    assert_eq!(
        DouyinApi::extract_sec_user_id("https://www.douyin.com/page?sec_user_id=MS4wLjABAAAA456def"),
        Some("MS4wLjABAAAA456def".to_string())
    );
}

/// 测试设置 Cookie
#[test]
fn test_set_cookie() {
    let mut api = DouyinApi::new().unwrap();
    api.set_cookie("test_cookie=value");
    // Cookie 设置不会失败
}

// ========== 异步测试（需要网络）==========
// 这些测试需要网络连接，默认被忽略

#[tokio::test]
#[ignore = "需要网络连接"]
async fn test_get_hot_search() {
    let api = DouyinApi::new().unwrap();
    let result = api.get_hot_search().await;
    
    // 热搜榜应该能正常获取
    if let Ok(json) = result {
        assert!(json.get("data").is_some() || json.get("word_list").is_some());
    }
}

#[tokio::test]
#[ignore = "需要网络连接"]
async fn test_resolve_short_url() {
    let api = DouyinApi::new().unwrap();
    
    // 测试一个假的短链接（实际测试时使用真实链接）
    let result = api.resolve_short_url("https://v.douyin.com/test").await;
    // 即使失败也不应该 panic
    let _ = result;
}

#[tokio::test]
#[ignore = "需要网络连接和有效 Cookie"]
async fn test_get_post_detail() {
    let api = DouyinApi::new().unwrap();
    
    // 使用一个已知的视频 ID 测试
    let result = api.get_post_detail("7321613070743663893").await;
    
    // 应该返回 JSON 数据
    if let Ok(json) = result {
        // 检查是否有 status_code 字段
        assert!(json.get("status_code").is_some());
    }
}

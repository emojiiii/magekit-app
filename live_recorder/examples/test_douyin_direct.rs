use reqwest::Client;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 构建参数
    let web_rid = "313899056971";
    let params_ordered = vec![
        ("aid", "6383"),
        ("app_name", "douyin_web"),
        ("live_id", "1"),
        ("device_platform", "web"),
        ("language", "zh-CN"),
        ("browser_language", "zh-CN"),
        ("browser_platform", "Win32"),
        ("browser_name", "Chrome"),
        ("browser_version", "116.0.0.0"),
        ("web_rid", web_rid),
        ("msToken", ""),
    ];

    let query_string: String = params_ordered
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
        .collect::<Vec<String>>()
        .join("&");

    // User-Agent 必须与签名使用的一致
    let user_agent = "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.5845.97 Safari/537.36 Core/1.116.567.400 QQBrowser/19.7.6764.400";
    
    // 生成签名 - 保留末尾的 '='
    let a_bogus = xbogus::ab_sign(&query_string, user_agent);
    
    let api_url = format!(
        "https://live.douyin.com/webcast/room/web/enter/?{}&a_bogus={}",
        query_string,
        a_bogus
    );
    
    println!("🌐 URL: {}", api_url);
    
    // 使用简化 Cookie
    let cookie = "ttwid=1%7C2iDIYVmjzMcpZ20fcaFde0VghXAA3NaNXE_SLR68IyE%7C1761045455%7Cab35197d5cfb21df6cbb2fa7ef1c9262206b062c315b9d04da746d0b37dfbc7d";
    
    // 不使用代理
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .no_proxy()  // 禁用代理
        .build()?;
    
    println!("📤 发送请求...");
    
    let response = client
        .get(&api_url)
        .header("Accept", "application/json, text/plain, */*")
        .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
        .header("Cookie", cookie)
        .header("Referer", format!("https://live.douyin.com/{}", web_rid))
        .header("User-Agent", user_agent)
        .send()
        .await?;
    
    println!("📡 状态: {}", response.status());
    println!("📋 Headers: {:?}", response.headers());
    
    let body = response.text().await?;
    println!("📄 响应长度: {}", body.len());
    if !body.is_empty() {
        println!("📄 响应: {}", &body[..body.len().min(500)]);
    } else {
        println!("❌ 响应为空!");
    }
    
    Ok(())
}

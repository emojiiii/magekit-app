//! Chrome DevTools Protocol (CDP) 监听模块

use crate::ad_detector::AdDetector;
use crate::filter::ResourceFilter;
use crate::types::{CapturedResource, ResourceType};
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::{Mutex, mpsc};
use tokio::time::Duration;
use tokio_tungstenite::connect_async;

const DEVTOOLS_HOST: &str = "127.0.0.1";
// 需要透传的 Header 白名单（小写）
const HEADER_WHITELIST: &[&str] = &[
    "referer",
    "user-agent",
    "cookie",
    "origin",
    "authorization",
    "accept-language",
];

/// CDP 监听器
pub struct CdpListener {
    client: reqwest::Client,
    filter: ResourceFilter,
    ad_detector: AdDetector,
    auto_speedup: bool,
    speedup_rate: f64,
}

impl CdpListener {
    /// 创建新的 CDP 监听器
    pub fn new(client: reqwest::Client, filter: ResourceFilter) -> Self {
        Self {
            client,
            filter,
            ad_detector: AdDetector::new(),
            auto_speedup: false,
            speedup_rate: 2.0,
        }
    }

    /// 设置自动加速播放
    pub fn with_auto_speedup(mut self, enabled: bool, rate: f64) -> Self {
        self.auto_speedup = enabled;
        self.speedup_rate = rate.max(1.0).min(16.0); // 限制在 1x-16x
        self
    }

    /// 开始监听 CDP 事件
    pub async fn listen(
        &mut self,
        devtools_port: u16,
        sent_urls: Arc<Mutex<std::collections::HashSet<String>>>,
        event_tx: mpsc::Sender<crate::types::CaptureEvent>,
        cancel_flag: Arc<AtomicBool>,
        page_title: Arc<Mutex<Option<String>>>,
        request_headers: Arc<Mutex<HashMap<String, Vec<(String, String)>>>>,
        referer: String,
    ) -> Result<()> {
        if devtools_port == 0 {
            let _ = event_tx
                .send(crate::types::CaptureEvent::Log(
                    "⚠️ 未找到可用调试端口，跳过 CDP 监听".into(),
                ))
                .await;
            return Ok(());
        }

        let ws_url = wait_for_ws_url(&self.client, devtools_port).await?;
        let (ws_stream, _) = connect_async(ws_url)
            .await
            .context("连接 DevTools WebSocket 失败")?;
        let (mut write, mut read) = ws_stream.split();

        // 启用 Network
        let enable_msg = serde_json::json!({
            "id": 1,
            "method": "Network.enable",
            "params": {
                "maxResourceBufferSize": 0,
                "maxTotalBufferSize": 0,
            }
        })
        .to_string();
        write
            .send(tokio_tungstenite::tungstenite::Message::Text(enable_msg))
            .await?;

        // 获取页面标题
        let title_eval_msg = serde_json::json!({
            "id": 2,
            "method": "Runtime.evaluate",
            "params": {
                "expression": "document.title",
                "returnByValue": true
            }
        })
        .to_string();
        write
            .send(tokio_tungstenite::tungstenite::Message::Text(title_eval_msg))
            .await?;

        // 隐藏 webdriver 特征（绕过 Cloudflare 检测）
        let hide_webdriver_msg = serde_json::json!({
            "id": 3,
            "method": "Page.addScriptToEvaluateOnNewDocument",
            "params": {
                "source": r#"
                (function() {
                    // 删除 webdriver 标志
                    Object.defineProperty(navigator, 'webdriver', {
                        get: () => undefined
                    });
                    
                    // 修改 chrome 对象
                    window.chrome = {
                        runtime: {}
                    };
                    
                    // 修改 permissions
                    const originalQuery = window.navigator.permissions.query;
                    window.navigator.permissions.query = (parameters) => (
                        parameters.name === 'notifications' ?
                            Promise.resolve({ state: Notification.permission }) :
                            originalQuery(parameters)
                    );
                    
                    // 修改 plugins
                    Object.defineProperty(navigator, 'plugins', {
                        get: () => [1, 2, 3, 4, 5]
                    });
                    
                    // 修改 languages
                    Object.defineProperty(navigator, 'languages', {
                        get: () => ['zh-CN', 'zh', 'en-US', 'en']
                    });
                    
                    // 修改 platform
                    Object.defineProperty(navigator, 'platform', {
                        get: () => 'Win32'
                    });
                    
                    // 覆盖 toString 方法
                    const getParameter = WebGLRenderingContext.prototype.getParameter;
                    WebGLRenderingContext.prototype.getParameter = function(parameter) {
                        if (parameter === 37445) {
                            return 'Intel Inc.';
                        }
                        if (parameter === 37446) {
                            return 'Intel Iris OpenGL Engine';
                        }
                        return getParameter.call(this, parameter);
                    };
                })();
                "#
            }
        })
        .to_string();
        write
            .send(tokio_tungstenite::tungstenite::Message::Text(hide_webdriver_msg))
            .await?;

        let mut request_id_url: HashMap<String, String> = HashMap::new();

        while let Some(msg) = read.next().await {
            if cancel_flag.load(Ordering::Relaxed) {
                break;
            }
            let msg = match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(t)) => t,
                _ => continue,
            };
            let v: Value = match serde_json::from_str(&msg) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // 捕获标题的响应
            if v.get("id").and_then(|i| i.as_i64()) == Some(2) {
                if let Some(val) = v
                    .get("result")
                    .and_then(|r| r.get("result"))
                    .and_then(|r| r.get("value"))
                    .and_then(|v| v.as_str())
                {
                    let mut guard = page_title.lock().await;
                    *guard = Some(val.to_string());
                }
                continue;
            }

            if let Some(method) = v.get("method").and_then(|m| m.as_str()) {
                match method {
                    "Network.requestWillBeSent" => {
                        if let Some(params) = v.get("params") {
                            if let Some(req_id) = params.get("requestId").and_then(|r| r.as_str()) {
                                if let Some(url) = params
                                    .get("request")
                                    .and_then(|r| r.get("url"))
                                    .and_then(|u| u.as_str())
                                {
                                    request_id_url.insert(req_id.to_string(), url.to_string());
                                }
                            }
                        }
                    }
                    "Network.requestWillBeSentExtraInfo" => {
                        if let Some(params) = v.get("params") {
                            if let Some(req_id) = params.get("requestId").and_then(|r| r.as_str()) {
                                if let Some(headers) = extract_headers(params.get("headers")) {
                                    let mut guard = request_headers.lock().await;
                                    guard.insert(req_id.to_string(), headers);
                                    // 防止过度增长
                                    if guard.len() > 2000 {
                                        guard.clear();
                                    }
                                }
                            }
                        }
                    }
                    "Network.responseReceived" => {
                        if let Some(params) = v.get("params") {
                            let request_id = params.get("requestId").and_then(|r| r.as_str());
                            let mut request_headers_for_id = None;
                            if let Some(req_id) = request_id {
                                let mut guard = request_headers.lock().await;
                                request_headers_for_id = guard.remove(req_id);
                            }

                            let url = params
                                .get("response")
                                .and_then(|r| r.get("url"))
                                .and_then(|u| u.as_str())
                                .map(|u| u.to_string())
                                .or_else(|| {
                                    request_id.and_then(|id| request_id_url.remove(id))
                                });
                            let mime = params
                                .get("response")
                                .and_then(|r| r.get("mimeType"))
                                .and_then(|m| m.as_str());
                            let content_length = params
                                .get("response")
                                .and_then(|r| r.get("headers"))
                                .and_then(|h| h.as_object())
                                .and_then(|h| h.get("content-length"))
                                .and_then(|v| v.as_str())
                                .and_then(|s| s.parse::<u64>().ok());

                            if let Some(url) = url {
                                let resource_type = if let Some(mime) = mime {
                                    ResourceType::from_mime_type(mime)
                                } else {
                                    ResourceType::from_url(&url)
                                };

                                // 检查是否匹配筛选器
                                let resource = CapturedResource {
                                    url: url.clone(),
                                    resource_type,
                                    mime_type: mime.map(|s| s.to_string()),
                                    referer: Some(referer.clone()),
                                    title: page_title.lock().await.clone(),
                                    size_bytes: content_length,
                                    duration_seconds: None, // CDP 不直接提供时长，需要后续解析
                                    headers: build_headers(request_headers_for_id, &referer),
                                };

                                if self.filter.matches(&resource) {
                                    // 检测广告并自动加速播放
                                    let is_ad = self.ad_detector.is_ad(&resource);
                                    if is_ad && self.auto_speedup {
                                        // 通过 CDP 加速播放视频
                                        let speedup_rate = self.speedup_rate;
                                        if let Err(e) = set_video_playback_speed(&mut write, speedup_rate).await {
                                            let _ = event_tx
                                                .send(crate::types::CaptureEvent::Log(format!(
                                                    "⚠️ 设置播放速度失败: {}",
                                                    e
                                                )))
                                                .await;
                                        } else {
                                            let _ = event_tx
                                                .send(crate::types::CaptureEvent::Log(format!(
                                                    "🚀 检测到广告，自动加速播放至 {:.1}x",
                                                    speedup_rate
                                                )))
                                                .await;
                                        }
                                    }

                                    push_found(
                                        resource,
                                        sent_urls.clone(),
                                        event_tx.clone(),
                                    )
                                    .await;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }
}

fn extract_headers(v: Option<&Value>) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    if let Some(obj) = v.and_then(|vv| {
        vv.get("headers")
            .and_then(|h| h.as_object())
            .or_else(|| vv.as_object())
    }) {
        for (k, v) in obj {
            if let Some(val) = v.as_str() {
                out.push((k.clone(), val.to_string()));
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn canonical_header_name(key_lower: &str) -> String {
    match key_lower {
        "user-agent" => "User-Agent".into(),
        "referer" => "Referer".into(),
        "cookie" => "Cookie".into(),
        "origin" => "Origin".into(),
        "authorization" => "Authorization".into(),
        "accept-language" => "Accept-Language".into(),
        _ => key_lower.to_string(),
    }
}

fn header_value(headers: Option<&Vec<(String, String)>>, key: &str) -> Option<String> {
    headers.and_then(|list| {
        list.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.clone())
    })
}

fn build_headers(
    request_headers: Option<Vec<(String, String)>>,
    fallback_referer: &str,
) -> Option<Vec<(String, String)>> {
    let headers_ref = request_headers.as_ref();
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    const CAPTURE_USER_AGENT: &str =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
    const CAPTURE_ACCEPT_LANGUAGE: &str = "zh-CN,zh;q=0.9,en;q=0.8";

    for key in HEADER_WHITELIST {
        match *key {
            "user-agent" => {
                let ua = header_value(headers_ref, "user-agent")
                    .unwrap_or_else(|| CAPTURE_USER_AGENT.to_string());
                if seen.insert("user-agent".into()) {
                    out.push((canonical_header_name("user-agent"), ua));
                }
            }
            "referer" => {
                let referer_val = header_value(headers_ref, "referer")
                    .unwrap_or_else(|| fallback_referer.to_string());
                if !referer_val.is_empty() && seen.insert("referer".into()) {
                    out.push((canonical_header_name("referer"), referer_val));
                }
            }
            "cookie" => {
                if let Some(cookie) = header_value(headers_ref, "cookie") {
                    if seen.insert("cookie".into()) {
                        out.push((canonical_header_name("cookie"), cookie));
                    }
                }
            }
            "origin" => {
                if let Some(origin) = header_value(headers_ref, "origin") {
                    if seen.insert("origin".into()) {
                        out.push((canonical_header_name("origin"), origin));
                    }
                }
            }
            other => {
                if let Some(val) = header_value(headers_ref, other) {
                    if seen.insert(other.to_string()) {
                        out.push((canonical_header_name(other), val));
                    }
                }
            }
        }
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

async fn push_found(
    resource: CapturedResource,
    sent_urls: Arc<Mutex<std::collections::HashSet<String>>>,
    event_tx: mpsc::Sender<crate::types::CaptureEvent>,
) {
    let mut guard = sent_urls.lock().await;
    if guard.insert(resource.url.clone()) {
        drop(guard);
        let _ = event_tx.send(crate::types::CaptureEvent::Found(resource)).await;
    }
}

async fn wait_for_ws_url(client: &reqwest::Client, port: u16) -> Result<String> {
    let endpoint_version = format!("http://{}:{}/json/version", DEVTOOLS_HOST, port);
    let endpoint_list = format!("http://{}:{}/json", DEVTOOLS_HOST, port);
    for _ in 0..30 {
        // 优先取 page target
        if let Ok(resp) = client.get(&endpoint_list).send().await {
            if let Ok(text) = resp.text().await {
                if let Ok(arr) = serde_json::from_str::<Vec<Value>>(&text) {
                    if let Some(ws) = arr
                        .iter()
                        .filter(|v| v.get("type").and_then(|t| t.as_str()) == Some("page"))
                        .filter_map(|v| v.get("webSocketDebuggerUrl").and_then(|u| u.as_str()))
                        .next()
                    {
                        return Ok(ws.to_string());
                    }
                    // 退而求其次取列表里的第一个 ws
                    if let Some(ws) = arr
                        .iter()
                        .filter_map(|v| v.get("webSocketDebuggerUrl").and_then(|u| u.as_str()))
                        .next()
                    {
                        return Ok(ws.to_string());
                    }
                }
            }
        }
        // 最后再尝试 browser 级 ws
        if let Ok(resp) = client.get(&endpoint_version).send().await {
            if let Ok(text) = resp.text().await {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    if let Some(ws) = v.get("webSocketDebuggerUrl").and_then(|u| u.as_str()) {
                        return Ok(ws.to_string());
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Err(anyhow::anyhow!("获取 DevTools WebSocket 地址失败"))
}

/// 通过 CDP 设置视频播放速度
async fn set_video_playback_speed<W>(write: &mut W, speed: f64) -> Result<()>
where
    W: futures_util::SinkExt<tokio_tungstenite::tungstenite::Message> + Unpin,
    <W as futures_util::Sink<tokio_tungstenite::tungstenite::Message>>::Error: std::fmt::Display,
{
    // JavaScript 代码：找到所有 video 元素并设置播放速度
    let js_code = format!(
        r#"
        (function() {{
            const videos = document.querySelectorAll('video');
            let count = 0;
            videos.forEach(video => {{
                if (video.readyState >= 2) {{ // HAVE_CURRENT_DATA
                    video.playbackRate = {};
                    count++;
                }}
            }});
            return count;
        }})()
        "#,
        speed
    );

    let speedup_msg = serde_json::json!({
        "id": 100,
        "method": "Runtime.evaluate",
        "params": {
            "expression": js_code,
            "returnByValue": true
        }
    })
    .to_string();

    write
        .send(tokio_tungstenite::tungstenite::Message::Text(speedup_msg))
        .await
        .map_err(|e| anyhow::anyhow!("发送 CDP 消息失败: {}", e))?;

    Ok(())
}

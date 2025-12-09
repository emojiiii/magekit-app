//! M3U8 抓取与下载能力
//!
//! - 负责浏览器探测、嗅探任务启动与事件流转
//! - UI 通过 `CaptureEvent` 订阅进度/日志

use crate::app::AppState;
use anyhow::{Context, Result};
use magekit_shared::utils::resolve_browser_path;
use rand::{Rng, distributions::Alphanumeric};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time;
use url::Url;

/// 抓取请求
#[derive(Debug, Clone)]
pub struct CaptureRequest {
    /// 目标页面 URL
    pub target_url: String,
    /// 自定义浏览器路径（优先级最高）
    pub custom_browser_path: Option<PathBuf>,
    /// 是否启用无头模式
    pub headless: bool,
    /// 抓取超时
    pub timeout: Duration,
}

/// 抓到的 m3u8 流
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct M3u8Stream {
    pub url: String,
    pub mime_type: Option<String>,
    pub referer: Option<String>,
    pub title: Option<String>,
}

/// 抓取事件
#[derive(Debug, Clone)]
pub enum CaptureEvent {
    Log(String),
    Found(M3u8Stream),
    Finished,
    Error(String),
}

/// 抓取会话句柄
pub struct CaptureSession {
    pub rx: mpsc::Receiver<CaptureEvent>,
    cancel_tx: Option<oneshot::Sender<()>>,
}

impl CaptureSession {
    pub fn cancel(&mut self) {
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(());
        }
    }

    pub fn split(self) -> (mpsc::Receiver<CaptureEvent>, Option<oneshot::Sender<()>>) {
        (self.rx, self.cancel_tx)
    }
}

impl AppState {
    /// 启动 m3u8 嗅探任务
    pub async fn start_m3u8_capture(&self, request: CaptureRequest) -> Result<CaptureSession> {
        let (event_tx, event_rx) = mpsc::channel(200);
        let (cancel_tx, mut cancel_rx) = oneshot::channel();

        let client = reqwest::Client::builder()
            .user_agent("MageKit/1.0 (m3u8-sniffer)")
            .redirect(reqwest::redirect::Policy::limited(10))
            .timeout(Duration::from_secs(20))
            .build()
            .context("创建 HTTP 客户端失败")?;

        let target_url = request.target_url.clone();
        let timeout = request.timeout;
        let headless = request.headless;
        let browser_path = resolve_browser_path(request.custom_browser_path.clone());

        // 在 Tokio runtime 上运行
        self.runtime.spawn(async move {
            let mut sent = HashSet::new();

            let _ = event_tx
                .send(CaptureEvent::Log(format!(
                    "🚀 开始嗅探: {} (headless={})",
                    target_url, headless
                )))
                .await;

            if let Some(path) = browser_path.clone() {
                let _ = event_tx
                    .send(CaptureEvent::Log(format!(
                        "🖥️ 浏览器路径: {}",
                        path.display()
                    )))
                    .await;
            } else {
                let _ = event_tx
                    .send(CaptureEvent::Log(
                        "⚠️ 未找到浏览器，将仅进行静态扫描（建议手动指定浏览器路径）".into(),
                    ))
                    .await;
            }

            let work = async {
                // 1) 快速静态扫描（不依赖浏览器）
                match quick_scan_for_m3u8(&client, &target_url).await {
                    Ok(found) => {
                        for item in found {
                            if sent.insert(item.clone()) {
                                let _ = event_tx.send(CaptureEvent::Found(item)).await;
                            }
                        }
                    }
                    Err(err) => {
                        let _ = event_tx
                            .send(CaptureEvent::Log(format!("⚠️ 静态扫描失败: {}", err)))
                            .await;
                    }
                }

                // 2) 预留：CDP 抓包逻辑（目前仅占位）
                if browser_path.is_some() {
                    let _ = event_tx
                        .send(CaptureEvent::Log(
                            "ℹ️ CDP 抓包未集成，后续将通过 remote debugging 捕获动态请求".into(),
                        ))
                        .await;
                }

                let _ = event_tx.send(CaptureEvent::Finished).await;
            };

            tokio::select! {
                _ = &mut cancel_rx => {
                    let _ = event_tx.send(CaptureEvent::Log("⏹️ 嗅探已取消".into())).await;
                }
                result = time::timeout(timeout, work) => {
                    if result.is_err() {
                        let _ = event_tx
                            .send(CaptureEvent::Log("⏰ 抓取已超时".into()))
                            .await;
                    };
                }
            };
        });

        Ok(CaptureSession {
            rx: event_rx,
            cancel_tx: Some(cancel_tx),
        })
    }
}

// =========================================================================================
// 辅助函数
// =========================================================================================

async fn quick_scan_for_m3u8(
    client: &reqwest::Client,
    target_url: &str,
) -> Result<Vec<M3u8Stream>> {
    let mut results = Vec::new();
    let url = Url::parse(target_url).context("URL 不合法")?;

    // 先抓取 HTML，用于提取 title
    let page_title = fetch_html_title(client, target_url).await.unwrap_or(None);

    // 1) HEAD 检查
    if let Ok(resp) = client.head(url.clone()).send().await {
        if let Some(mt) = resp.headers().get(reqwest::header::CONTENT_TYPE) {
            if let Ok(mt) = mt.to_str() {
                if mt.contains("mpegurl") || mt.contains("vnd.apple.mpegurl") {
                    results.push(M3u8Stream {
                        url: target_url.to_string(),
                        mime_type: Some(mt.to_string()),
                        referer: None,
                        title: page_title.clone(),
                    });
                    return Ok(results);
                }
            }
        }
    }

    // 2) GET 内容扫描
    let body = client
        .get(url.clone())
        .send()
        .await
        .context("获取页面失败")?
        .text()
        .await
        .context("读取页面内容失败")?;

    let mut seen = HashSet::new();
    for token in
        body.split(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '<' || c == '>')
    {
        if token.contains(".m3u8") {
            let candidate = token.trim_matches(['"', '\'', ',', ';']);
            if candidate.len() < 5 {
                continue;
            }

            let absolute = if let Ok(u) = Url::parse(candidate) {
                u
            } else if let Ok(joined) = url.join(candidate) {
                joined
            } else {
                continue;
            };

            let final_url = absolute.to_string();
            if seen.insert(final_url.clone()) {
                results.push(M3u8Stream {
                    url: final_url,
                    mime_type: None,
                    referer: Some(target_url.to_string()),
                    title: page_title.clone(),
                });
            }
        }
    }

    Ok(results)
}

async fn fetch_html_title(client: &reqwest::Client, target_url: &str) -> Result<Option<String>> {
    let body = client
        .get(target_url)
        .send()
        .await
        .context("获取页面失败")?
        .text()
        .await
        .context("读取页面内容失败")?;

    if let Some(start) = body.to_lowercase().find("<title>") {
        if let Some(end) = body.to_lowercase().find("</title>") {
            if end > start + 7 {
                let title_raw = &body[start + 7..end];
                let title = title_raw.trim().replace('\n', " ").replace('\r', " ");
                return Ok(Some(title));
            }
        }
    }
    Ok(None)
}

/// 为 CDP 会话创建临时用户数据目录
#[allow(dead_code)]
fn make_temp_user_data_dir() -> PathBuf {
    let random_suffix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();

    std::env::temp_dir().join(format!("magekit-profile-{}", random_suffix))
}

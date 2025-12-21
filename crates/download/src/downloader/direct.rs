use std::collections::VecDeque;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use crate::config::DownloadRequest;
use crate::error::{DownloadError, DownloadResult};
use crate::progress::{DownloadCallback, DownloadOutcome, DownloadProgress};
use crate::utils::{part_path_for_output, resolve_output_path};

/// 直链 HTTP 下载
pub struct DirectDownloader;

impl Default for DirectDownloader {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl crate::downloader::Downloader for DirectDownloader {
    fn name(&self) -> &'static str {
        "direct"
    }

    async fn download(
        &self,
        request: DownloadRequest,
        callback: &dyn DownloadCallback,
        cancel: CancellationToken,
    ) -> DownloadResult<DownloadOutcome> {
        callback.on_progress(DownloadProgress::preparing());

        // 构建 HTTP 客户端
        let mut client_builder = reqwest::Client::builder();
        if let Some(timeout) = request.timeout {
            client_builder = client_builder.timeout(timeout);
        }
        let client = client_builder
            .build()
            .map_err(|e| DownloadError::Internal(format!("HTTP client build failed: {}", e)))?;

        // 构建请求
        let mut url = request.url.clone();
        if !request.extra.query.is_empty() {
            let mut pairs = url.query_pairs().into_owned().collect::<Vec<_>>();
            pairs.extend(request.extra.query.clone());
            url.query_pairs_mut().clear().extend_pairs(pairs);
        }

        let mut req = client.get(url.clone());
        for (k, v) in &request.extra.headers {
            req = req.header(k, v);
        }
        if let Some(cookie) = &request.extra.cookie {
            req = req.header(reqwest::header::COOKIE, cookie);
        }

        // 输出路径与临时文件
        let output_path = resolve_output_path(&request, &url)?;
        let temp_path = part_path_for_output(&output_path);
        if let Some(dir) = output_path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }

        // 断点续传：检测已有临时文件
        let mut downloaded: u64 = 0;
        let mut created_new = false;
        let mut file = if request.resume {
            match tokio::fs::metadata(&temp_path).await {
                Ok(meta) => {
                    downloaded = meta.len();
                    tokio::fs::OpenOptions::new()
                        .append(true)
                        .open(&temp_path)
                        .await?
                }
                Err(_) => {
                    created_new = true;
                    tokio::fs::OpenOptions::new()
                        .create(true)
                        .write(true)
                        .open(&temp_path)
                        .await?
                }
            }
        } else {
            let _ = tokio::fs::remove_file(&temp_path).await;
            created_new = true;
            tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_path)
                .await?
        };

        if downloaded > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={}-", downloaded));
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                // 超时/网络错误：若本次新建的 .part 仍为空，清理避免堆积垃圾文件
                if created_new && downloaded == 0 {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                }
                return Err(DownloadError::from(e));
            }
        };
        if !resp.status().is_success() {
            if created_new && downloaded == 0 {
                let _ = tokio::fs::remove_file(&temp_path).await;
            }
            return Err(DownloadError::Network(format!("HTTP {}", resp.status())));
        }

        // 如果服务器不支持 Range（返回 200），且有已下载，重下
        if downloaded > 0 && resp.status() == reqwest::StatusCode::OK {
            downloaded = 0;
            file = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_path)
                .await?;
        }

        // 计算 total：优先 Content-Range，总长包含已下部分
        let total = parse_content_range_total(resp.headers())
            .or_else(|| resp.content_length().map(|len| len + downloaded));
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let content_type_clone = content_type.clone();

        // 下载流
        let mut stream = resp.bytes_stream();
        let _started = Instant::now();
        let mut speed_window: VecDeque<(Instant, u64)> = VecDeque::new(); // (time, bytes)
        let speed_span = Duration::from_secs(5);

        while let Some(chunk) = stream.next().await {
            if cancel.is_cancelled() {
                return Err(DownloadError::Canceled);
            }

            let chunk = chunk.map_err(DownloadError::from)?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            // 平滑速度：5 秒窗口平均
            let now = Instant::now();
            speed_window.push_back((now, chunk.len() as u64));
            while let Some((t, _)) = speed_window.front() {
                if now.duration_since(*t) > speed_span {
                    speed_window.pop_front();
                } else {
                    break;
                }
            }
            let speed_bytes: u64 = speed_window.iter().map(|(_, b)| *b).sum();
            let speed = speed_bytes
                .checked_div(
                    now.duration_since(speed_window.front().map(|(t, _)| *t).unwrap_or(now))
                        .as_secs()
                        .max(1),
                )
                .or(Some(0));

            callback.on_progress(DownloadProgress::downloading(downloaded, total, speed));
        }

        file.flush().await?;

        // 校验：避免把 HTML/JSON/m3u8 等页面内容当作 mp4 保存
        // 不做额外网络请求，仅在下载完成后读取本地文件头判断。
        if created_new {
            let mut f = tokio::fs::File::open(&temp_path).await?;
            let mut buf = [0u8; 512];
            let n = f.read(&mut buf).await.unwrap_or(0);
            let head = &buf[..n];

            let trimmed = {
                let mut i = 0usize;
                while i < head.len() && head[i].is_ascii_whitespace() {
                    i += 1;
                }
                if head.len() >= i + 3 && head[i..i + 3] == [0xEF, 0xBB, 0xBF] {
                    i += 3;
                }
                &head[i..]
            };

            let ct = content_type.as_deref().unwrap_or("");
            let looks_like_m3u8 = trimmed.starts_with(b"#EXTM3U")
                || ct.contains("mpegurl")
                || ct.contains("m3u8");
            let looks_like_html = trimmed.starts_with(b"<") || ct.contains("text/html");
            let looks_like_json = trimmed.starts_with(b"{") || ct.contains("application/json");

            if looks_like_m3u8 || looks_like_html || looks_like_json {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return Err(DownloadError::Network(format!(
                    "unexpected response for direct download (content-type={})",
                    ct
                )));
            }
        }

        // 成功后重命名
        tokio::fs::rename(&temp_path, &output_path).await?;
        callback.on_progress(DownloadProgress::completed(downloaded, total));

        callback.on_complete(DownloadOutcome {
            output_path: output_path.clone(),
            content_type: content_type_clone,
            details: Default::default(),
        });

        Ok(DownloadOutcome {
            output_path,
            content_type,
            details: Default::default(),
        })
    }
}

pub fn parse_content_range_total(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers.get(reqwest::header::CONTENT_RANGE).and_then(|v| {
        v.to_str().ok().and_then(|s| {
            // bytes start-end/total
            s.split('/').nth(1).and_then(|t| t.parse::<u64>().ok())
        })
    })
}

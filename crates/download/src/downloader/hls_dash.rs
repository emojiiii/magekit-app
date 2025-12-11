use std::time::Instant;

use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use async_trait::async_trait;
use futures_util::stream::{self, StreamExt};
use m3u8_rs::{KeyMethod, MasterPlaylist, Playlist, VariantStream};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::config::DownloadRequest;
use crate::downloader::ffmpeg::FfmpegDownloader;
use crate::error::{DownloadError, DownloadResult};
use crate::progress::{DownloadCallback, DownloadOutcome, DownloadProgress};
use crate::utils::resolve_output_path;

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// HLS/DASH 下载器：支持 m3u8 并发分段、AES-128 解密；mpd 则复用 ffmpeg
pub struct HlsDashDownloader;

impl Default for HlsDashDownloader {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl crate::downloader::Downloader for HlsDashDownloader {
    fn name(&self) -> &'static str {
        "hls_dash"
    }

    async fn download(
        &self,
        request: DownloadRequest,
        callback: &dyn DownloadCallback,
        cancel: CancellationToken,
    ) -> DownloadResult<DownloadOutcome> {
        callback.on_progress(DownloadProgress::preparing());

        let manifest_url = request.url.clone();
        // mpd 走 ffmpeg 拷贝（简化实现）
        if manifest_url.path().ends_with(".mpd") {
            let ff = FfmpegDownloader::default();
            return ff.download(request, callback, cancel).await;
        }

        // m3u8
        let manifest_text = fetch_text(&manifest_url, &request, &cancel).await?;
        let parsed = m3u8_rs::parse_playlist_res(manifest_text.as_bytes())
            .map_err(|e| DownloadError::Unsupported(format!("m3u8 parse error: {:?}", e)))?;

        let media_playlist = match parsed {
            Playlist::MasterPlaylist(master) => {
                let variant = pick_variant(&master)
                    .ok_or_else(|| DownloadError::Unsupported("no variant in master".into()))?;
                let uri = manifest_url
                    .join(variant.uri.as_str())
                    .map_err(|e| DownloadError::InvalidRequest(e.to_string()))?;
                let text = fetch_text(&uri, &request, &cancel).await?;
                let parsed_media = m3u8_rs::parse_playlist_res(text.as_bytes()).map_err(|e| {
                    DownloadError::Unsupported(format!("media m3u8 parse error: {:?}", e))
                })?;
                match parsed_media {
                    Playlist::MediaPlaylist(m) => (uri, m),
                    _ => return Err(DownloadError::Unsupported("expected media playlist".into())),
                }
            }
            Playlist::MediaPlaylist(m) => (manifest_url.clone(), m),
        };

        let (media_base, media) = media_playlist;
        let total_segments = media.segments.len() as u64;

        let output_path = resolve_output_path(&request, &request.url)?;
        if let Some(dir) = output_path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        let mut file = tokio::fs::File::create(&output_path).await?;

        // AES-128 key/iv（简单实现：使用当前 Key 配置）
        let mut current_key: Option<AesKey> = None;
        let mut segments = Vec::with_capacity(media.segments.len());
        for (idx, seg) in media.segments.iter().enumerate() {
            if let Some(key) = &seg.key {
                if matches!(key.method, KeyMethod::AES128) {
                    let key_uri = key
                        .uri
                        .as_ref()
                        .and_then(|u| media_base.join(u).ok())
                        .ok_or_else(|| {
                            DownloadError::Unsupported("AES-128 key uri missing".into())
                        })?;
                    let key_bytes = fetch_bytes(&key_uri, &request, &cancel).await?;
                    let iv = key
                        .iv
                        .as_ref()
                        .and_then(|s| hex::decode(s.trim_start_matches("0x")).ok())
                        .and_then(|v| {
                            let mut iv = [0u8; 16];
                            if v.len() == 16 {
                                iv.copy_from_slice(&v);
                                Some(iv)
                            } else {
                                None
                            }
                        })
                        .unwrap_or([0u8; 16]);
                    let mut key_arr = [0u8; 16];
                    key_arr.copy_from_slice(&key_bytes[..16.min(key_bytes.len())]);
                    current_key = Some(AesKey { key: key_arr, iv });
                } else {
                    current_key = None;
                }
            }

            let seg_uri = media_base
                .join(seg.uri.as_str())
                .map_err(|e| DownloadError::InvalidRequest(e.to_string()))?;
            segments.push((idx, seg_uri, current_key.clone()));
        }

        let concurrency = 6usize;
        let start = Instant::now();
        let mut downloaded_bytes = 0u64;
        let mut results = stream::iter(segments)
            .map(|(idx, uri, key)| {
                let req = &request;
                let cancel = cancel.clone();
                async move {
                    if cancel.is_cancelled() {
                        return Err(DownloadError::Canceled);
                    }
                    let mut bytes = fetch_bytes(&uri, req, &cancel).await?;
                    if let Some(k) = key {
                        bytes = decrypt_aes128(&bytes, &k)?;
                    }
                    Ok::<(usize, Vec<u8>), DownloadError>((idx, bytes))
                }
            })
            .buffer_unordered(concurrency)
            .collect::<Vec<_>>()
            .await;

        // 按序写入
        results.sort_by_key(|r| r.as_ref().ok().map(|(i, _)| *i).unwrap_or(usize::MAX));
        let mut written_segments = 0u64;
        for res in results {
            let (_idx, bytes) = res?;
            downloaded_bytes += bytes.len() as u64;
            file.write_all(&bytes).await?;
            written_segments += 1;

            let elapsed = start.elapsed().as_secs();
            let speed = if elapsed == 0 {
                None
            } else {
                Some(downloaded_bytes / elapsed)
            };
            callback.on_progress(DownloadProgress::downloading(
                written_segments,
                Some(total_segments),
                speed,
            ));
        }

        file.flush().await?;
        callback.on_progress(DownloadProgress::completed(
            written_segments,
            Some(total_segments),
        ));
        callback.on_complete(DownloadOutcome {
            output_path: output_path.clone(),
            content_type: None,
            details: Default::default(),
        });

        Ok(DownloadOutcome {
            output_path,
            content_type: None,
            details: Default::default(),
        })
    }
}

fn pick_variant(master: &MasterPlaylist) -> Option<&VariantStream> {
    master.variants.iter().max_by_key(|v| v.bandwidth)
}

async fn fetch_text(
    url: &url::Url,
    request: &DownloadRequest,
    cancel: &CancellationToken,
) -> DownloadResult<String> {
    let bytes = fetch_bytes(url, request, cancel).await?;
    String::from_utf8(bytes)
        .map_err(|e| DownloadError::Internal(format!("utf8 decode playlist: {}", e)))
}

async fn fetch_bytes(
    url: &url::Url,
    request: &DownloadRequest,
    cancel: &CancellationToken,
) -> DownloadResult<Vec<u8>> {
    let mut client_builder = reqwest::Client::builder();
    if let Some(timeout) = request.timeout {
        client_builder = client_builder.timeout(timeout);
    }
    let client = client_builder
        .build()
        .map_err(|e| DownloadError::Internal(format!("HTTP client build failed: {}", e)))?;

    let mut req = client.get(url.clone());
    for (k, v) in &request.extra.headers {
        req = req.header(k, v);
    }
    if let Some(cookie) = &request.extra.cookie {
        req = req.header(reqwest::header::COOKIE, cookie);
    }

    let resp = req.send().await.map_err(DownloadError::from)?;
    if !resp.status().is_success() {
        return Err(DownloadError::Network(format!("HTTP {}", resp.status())));
    }

    if cancel.is_cancelled() {
        return Err(DownloadError::Canceled);
    }

    let bytes = resp.bytes().await.map_err(DownloadError::from)?;
    Ok(bytes.to_vec())
}

#[derive(Clone)]
struct AesKey {
    key: [u8; 16],
    iv: [u8; 16],
}

fn decrypt_aes128(buf: &[u8], key: &AesKey) -> DownloadResult<Vec<u8>> {
    let decryptor = Aes128CbcDec::new(&key.key.into(), &key.iv.into());
    let mut buf_mut = buf.to_vec();
    let decrypted = decryptor
        .decrypt_padded_mut::<Pkcs7>(&mut buf_mut)
        .map_err(|e| DownloadError::Internal(format!("AES-128 decrypt: {:?}", e)))?;
    Ok(decrypted.to_vec())
}

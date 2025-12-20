use std::path::{Path, PathBuf};
use std::time::Instant;

use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use async_trait::async_trait;
use futures_util::stream::{self, StreamExt};
use m3u8_rs::{KeyMethod, MasterPlaylist, Playlist, VariantStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::config::DownloadRequest;
use crate::downloader::ffmpeg::FfmpegDownloader;
use crate::error::{DownloadError, DownloadResult};
use crate::progress::{DownloadCallback, DownloadOutcome, DownloadProgress, DownloadStage};
use crate::utils::{hls_cache_dir, resolve_output_path};

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// HLS/DASH 下载器：支持 m3u8 并发分段、AES-128 解密；mpd 则复用 ffmpeg
pub struct HlsDashDownloader {
    ffmpeg_path: PathBuf,
}

impl HlsDashDownloader {
    pub fn new(ffmpeg_path: PathBuf) -> Self {
        Self { ffmpeg_path }
    }
}

impl Default for HlsDashDownloader {
    fn default() -> Self {
        Self {
            ffmpeg_path: PathBuf::from("ffmpeg"),
        }
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
            let ff = FfmpegDownloader::new(self.ffmpeg_path.clone());
            return ff.download(request, callback, cancel).await;
        }

        // 构建 HTTP client（整个下载生命周期复用）
        let client = build_client(&request)?;

        // m3u8
        let manifest_text = fetch_text(&client, &manifest_url, &request, &cancel).await?;
        let parsed = m3u8_rs::parse_playlist_res(manifest_text.as_bytes())
            .map_err(|e| DownloadError::Unsupported(format!("m3u8 parse error: {:?}", e)))?;

        let media_playlist = match parsed {
            Playlist::MasterPlaylist(master) => {
                let variant = pick_variant(&master)
                    .ok_or_else(|| DownloadError::Unsupported("no variant in master".into()))?;
                let uri = manifest_url
                    .join(variant.uri.as_str())
                    .map_err(|e| DownloadError::InvalidRequest(e.to_string()))?;
                let text = fetch_text(&client, &uri, &request, &cancel).await?;
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

        // Live m3u8：不走下载模式（需要使用录制功能）
        if !media.end_list {
            return Err(DownloadError::Unsupported(
                "检测到直播 m3u8：请使用录制功能，不支持下载模式".into(),
            ));
        }

        // fMP4 / discontinuity / 非 AES-128 加密：交给 ffmpeg 处理，避免拼接/合并出错
        let has_map = media.segments.iter().any(|s| s.map.is_some());
        let has_discontinuity =
            media.segments.iter().any(|s| s.discontinuity) || media.discontinuity_sequence != 0;
        let has_unsupported_key = media.segments.iter().any(|s| {
            s.key
                .as_ref()
                .is_some_and(|k| !matches!(k.method, KeyMethod::None | KeyMethod::AES128))
        });

        if has_map || has_discontinuity || has_unsupported_key {
            let ff = FfmpegDownloader::new(self.ffmpeg_path.clone());
            return ff.download(request, callback, cancel).await;
        }

        let total_segments = media.segments.len() as u64;

        let output_path = resolve_output_path(&request, &request.url)?;
        if let Some(dir) = output_path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }

        // 独立缓存目录：用于分段下载/断点续传；成功后会自动清理
        let cache_dir = hls_cache_dir(&output_path, &request.url);
        tokio::fs::create_dir_all(&cache_dir).await?;

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
                    let key_bytes = fetch_bytes(&client, &key_uri, &request, &cancel).await?;
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
        let mut downloaded_segments = 0u64;
        let mut downloaded_bytes = 0u64;

        // 断点续传：复用已存在的分片文件
        if request.resume {
            for (idx, _uri, _key) in &segments {
                let p = segment_path(&cache_dir, *idx);
                if let Ok(meta) = tokio::fs::metadata(&p).await {
                    if meta.len() > 0 {
                        downloaded_segments += 1;
                        downloaded_bytes += meta.len();
                    }
                }
            }
        }

        // 仅下载缺失分片（resume 时复用已完成分片）
        let mut pending = Vec::new();
        for (idx, uri, key) in segments {
            if request.resume {
                let p = segment_path(&cache_dir, idx);
                if let Ok(meta) = tokio::fs::metadata(&p).await {
                    if meta.len() > 0 {
                        continue;
                    }
                }
            }
            pending.push((idx, uri, key));
        }

        let mut stream = stream::iter(pending)
            .map(|(idx, uri, key)| {
                let req = &request;
                let cancel = cancel.clone();
                let cache_dir = cache_dir.clone();
                let client = client.clone();
                async move {
                    if cancel.is_cancelled() {
                        return Err(DownloadError::Canceled);
                    }
                    let mut bytes = fetch_bytes(&client, &uri, req, &cancel).await?;
                    if let Some(k) = key {
                        bytes = decrypt_aes128(&bytes, &k)?;
                    }

                    let final_path = segment_path(&cache_dir, idx);
                    let tmp_path = final_path.with_extension("part");
                    if final_path.exists() {
                        let _ = tokio::fs::remove_file(&final_path).await;
                    }
                    tokio::fs::write(&tmp_path, &bytes).await?;
                    tokio::fs::rename(&tmp_path, &final_path).await?;
                    Ok::<(usize, u64), DownloadError>((idx, bytes.len() as u64))
                }
            })
            .buffer_unordered(concurrency);

        while let Some(res) = stream.next().await {
            let (_idx, bytes_len) = res?;
            downloaded_segments += 1;
            downloaded_bytes += bytes_len;

            let elapsed = start.elapsed().as_secs().max(1);
            let speed = Some(downloaded_bytes / elapsed);
            let total_bytes_est = if downloaded_segments == 0 {
                None
            } else {
                Some(
                    (downloaded_bytes as f64 * (total_segments as f64 / downloaded_segments as f64))
                        .ceil() as u64,
                )
            };

            callback.on_progress(DownloadProgress::downloading(
                downloaded_bytes,
                total_bytes_est,
                speed,
            ));
        }

        // 合并：输出为容器格式时优先走 ffmpeg；否则直接拼接写入（适配测试场景 out.ts）
        let output_ext = output_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let needs_mux = matches!(output_ext.as_str(), "mp4" | "mkv" | "avi");

        if needs_mux {
            callback.on_progress(crate::progress::DownloadProgress {
                stage: DownloadStage::Merging,
                bytes_downloaded: downloaded_bytes,
                total_bytes: None,
                speed_bps: None,
                eta: None,
            });
            merge_with_ffmpeg(
                &self.ffmpeg_path,
                &cache_dir,
                total_segments as usize,
                &output_path,
                &cancel,
            )
            .await?;
        } else {
            concat_to_file(&cache_dir, total_segments as usize, &output_path, &cancel).await?;
        }

        // 成功后清理缓存目录（用户期望：下载完成后自动删除缓存）
        let _ = tokio::fs::remove_dir_all(&cache_dir).await;

        callback.on_progress(DownloadProgress::completed(downloaded_bytes, None));
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

fn build_client(request: &DownloadRequest) -> DownloadResult<reqwest::Client> {
    let mut client_builder = reqwest::Client::builder();
    if let Some(timeout) = request.timeout {
        client_builder = client_builder.timeout(timeout);
    }
    client_builder
        .build()
        .map_err(|e| DownloadError::Internal(format!("HTTP client build failed: {}", e)))
}

async fn fetch_text(
    client: &reqwest::Client,
    url: &url::Url,
    request: &DownloadRequest,
    cancel: &CancellationToken,
) -> DownloadResult<String> {
    let bytes = fetch_bytes(client, url, request, cancel).await?;
    String::from_utf8(bytes)
        .map_err(|e| DownloadError::Internal(format!("utf8 decode playlist: {}", e)))
}

async fn fetch_bytes(
    client: &reqwest::Client,
    url: &url::Url,
    request: &DownloadRequest,
    cancel: &CancellationToken,
) -> DownloadResult<Vec<u8>> {
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

fn segment_path(cache_dir: &Path, idx: usize) -> PathBuf {
    cache_dir.join(format!("{:06}.ts", idx))
}

async fn concat_to_file(
    cache_dir: &Path,
    total_segments: usize,
    output_path: &Path,
    cancel: &CancellationToken,
) -> DownloadResult<()> {
    let mut out = tokio::fs::File::create(output_path).await?;
    for idx in 0..total_segments {
        if cancel.is_cancelled() {
            return Err(DownloadError::Canceled);
        }
        let p = segment_path(cache_dir, idx);
        let mut seg = tokio::fs::File::open(&p).await?;
        tokio::io::copy(&mut seg, &mut out).await?;
    }
    out.flush().await?;
    Ok(())
}

async fn merge_with_ffmpeg(
    ffmpeg_path: &Path,
    cache_dir: &Path,
    total_segments: usize,
    output_path: &Path,
    cancel: &CancellationToken,
) -> DownloadResult<()> {
    let concat_list = cache_dir.join("concat.txt");
    let mut list = String::new();
    for idx in 0..total_segments {
        let p = segment_path(cache_dir, idx);
        let path_str = p.to_string_lossy().replace('\\', "/");
        let escaped = path_str.replace('\'', "'\\''");
        list.push_str(&format!("file '{}'\n", escaped));
    }
    tokio::fs::write(&concat_list, list).await?;

    let out_file_name = output_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("output.mp4");
    let tmp_out = output_path.with_file_name(format!("{}.part", out_file_name));

    let mut cmd = Command::new(ffmpeg_path);
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-nostats")
        .arg("-y")
        .arg("-f")
        .arg("concat")
        .arg("-safe")
        .arg("0")
        .arg("-i")
        .arg(concat_list.to_string_lossy().to_string())
        .arg("-c")
        .arg("copy");

    // mp4 常见需要 aac_adtstoasc（HLS AAC）
    if output_path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("mp4"))
        .unwrap_or(false)
    {
        cmd.arg("-bsf:a").arg("aac_adtstoasc");
    }

    cmd.arg(tmp_out.to_string_lossy().to_string());
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| DownloadError::Internal(format!("spawn ffmpeg failed: {}", e)))?;

    let stderr_handle = {
        let stderr = child.stderr.take();
        tokio::spawn(async move {
            let mut buf = String::new();
            if let Some(stderr) = stderr {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    buf.push_str(&line);
                    buf.push('\n');
                }
            }
            buf
        })
    };

    tokio::select! {
        _ = cancel.cancelled() => {
            let _ = child.kill().await;
            stderr_handle.abort();
            return Err(DownloadError::Canceled);
        }
        status = child.wait() => {
            let status = status.map_err(|e| DownloadError::Internal(format!("wait ffmpeg failed: {}", e)))?;
            if !status.success() {
                let stderr_buf = stderr_handle.await.unwrap_or_default();
                return Err(DownloadError::ProcessExit { code: status.code(), stderr: stderr_buf });
            }
        }
    }

    if output_path.exists() {
        let _ = tokio::fs::remove_file(output_path).await;
    }
    tokio::fs::rename(&tmp_out, output_path).await?;
    Ok(())
}

//! 视频下载功能
//!
//! 包含视频信息获取和下载功能

use anyhow::Result;
use std::sync::Arc;

use super::state::AppState;
use super::types::DownloadVideoOptions;
use super::utils::{extract_percent, extract_total_size, extract_speed};

impl AppState {
    /// 获取视频信息 (在后台线程中运行)
    pub fn get_video_info_in_background(&self, url: String) -> std::thread::JoinHandle<Result<magekit_shared::VideoInfo>> {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();
        
        std::thread::spawn(move || {
            runtime.block_on(async {
                tool_manager.get_video_info(&url).await
                    .map_err(|e| anyhow::anyhow!("获取视频信息失败: {}", e))
            })
        })
    }

    /// 下载视频（在后台线程中运行，带进度回调）
    /// progress_callback: fn(progress_percent, speed_bytes_per_sec, downloaded_bytes, total_bytes)
    pub fn download_video_in_background(
        &self,
        url: String,
        output_dir: std::path::PathBuf,
        format_id: String,
        options: DownloadVideoOptions,
        progress_callback: Arc<dyn Fn(f32, u64, u64, u64) + Send + Sync>,
        _title: Option<String>,  // 保留备用
    ) -> std::thread::JoinHandle<Result<std::path::PathBuf>> {
        let storage = self.tool_manager.storage.clone();
        
        std::thread::spawn(move || {
            // 获取 yt-dlp 路径
            let yt_dlp_path = storage.get_tool_path(magekit_shared::ToolType::YtDlp);
            
            // 如果应用内没有安装，尝试使用系统的
            let yt_dlp_path = if yt_dlp_path.exists() {
                yt_dlp_path
            } else {
                which::which("yt-dlp")
                    .map_err(|_| anyhow::anyhow!("yt-dlp 未安装，请先在工具页面安装"))?
            };
            
            // 处理格式 ID
            let effective_format_id = if format_id.chars().all(|c| c.is_ascii_digit()) {
                format!("{}+bestaudio/best", format_id)
            } else if format_id.contains('+') || format_id.starts_with("best") {
                format_id.clone()
            } else {
                format!("{}+bestaudio/best", format_id)
            };
            
            tracing::info!("📝 原始格式ID: {}, 实际使用: {}", format_id, effective_format_id);
            
            // 文件名模板
            let output_template = output_dir.join("%(title)s_%(height)sp.%(ext)s");
            
            // 构建下载命令
            let mut cmd = std::process::Command::new(&yt_dlp_path);
            cmd.arg(&url)
               .arg("--format").arg(&effective_format_id)
               .arg("--output").arg(&output_template)
               .arg("--newline")
               .arg("--progress")
               .arg("--no-mtime");
            
            // 元数据选项
            if options.embed_metadata {
                cmd.arg("--embed-metadata");
            }
            if options.embed_thumbnail {
                cmd.arg("--embed-thumbnail");
            }
            
            // 音频选项
            if options.audio_only {
                cmd.arg("--extract-audio");
                cmd.arg("--audio-format").arg("mp3");
            }
            
            // 字幕选项
            if options.download_subtitles {
                cmd.arg("--write-subs");
                cmd.arg("--write-auto-subs");
            }
            
            // 获取 ffmpeg 路径
            let ffmpeg_path = storage.get_tool_path(magekit_shared::ToolType::Ffmpeg);
            if ffmpeg_path.exists() {
                cmd.arg("--ffmpeg-location").arg(&ffmpeg_path);
            } else if let Ok(system_ffmpeg) = which::which("ffmpeg") {
                cmd.arg("--ffmpeg-location").arg(system_ffmpeg);
            }
            
            // 启动进程
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());
            
            tracing::info!("🚀 启动 yt-dlp 命令: {:?}", cmd);
            
            let mut child = cmd.spawn()
                .map_err(|e| anyhow::anyhow!("启动 yt-dlp 失败: {}", e))?;
            
            let stdout = child.stdout.take()
                .ok_or_else(|| anyhow::anyhow!("无法获取 stdout"))?;
            let stderr = child.stderr.take()
                .ok_or_else(|| anyhow::anyhow!("无法获取 stderr"))?;
            
            use std::io::BufRead;
            let stdout_reader = std::io::BufReader::new(stdout);
            let stderr_reader = std::io::BufReader::new(stderr);
            
            let mut output_file: Option<std::path::PathBuf> = None;
            let mut all_lines: Vec<String> = Vec::new();
            let mut last_total: u64 = 0;
            
            // 在单独线程中读取 stderr
            let stderr_handle = std::thread::spawn(move || {
                let mut stderr_lines: Vec<String> = Vec::new();
                for line in stderr_reader.lines() {
                    if let Ok(line) = line {
                        tracing::warn!("[yt-dlp stderr] {}", line);
                        stderr_lines.push(line);
                    }
                }
                stderr_lines
            });
            
            // 读取 stdout
            for line in stdout_reader.lines() {
                if let Ok(line) = line {
                    tracing::info!("[yt-dlp] {}", line);
                    all_lines.push(line.clone());
                    
                    // 解析进度信息
                    if line.contains("[download]") && line.contains("%") && !line.contains("Destination") {
                        let percent = extract_percent(&line);
                        let total = extract_total_size(&line);
                        let speed = extract_speed(&line);
                        
                        if total > 0 {
                            last_total = total;
                        }
                        let effective_total = if total > 0 { total } else { last_total };
                        let downloaded = ((percent / 100.0) * effective_total as f32) as u64;
                        
                        tracing::info!("📊 进度: {:.1}%, 速度: {} B/s, 已下载: {} B, 总大小: {} B", 
                            percent, speed, downloaded, effective_total);
                        progress_callback(percent / 100.0, speed, downloaded, effective_total);
                    }
                    
                    // 检测输出文件路径
                    if line.contains("Destination:") {
                        if let Some(path) = line.split("Destination:").nth(1) {
                            let path = path.trim();
                            tracing::info!("📁 检测到输出路径 (Destination): {}", path);
                            output_file = Some(std::path::PathBuf::from(path));
                        }
                    }
                    
                    if line.contains("Merging formats into") {
                        if let Some(start) = line.find('"') {
                            if let Some(end) = line.rfind('"') {
                                if end > start {
                                    let path = &line[start + 1..end];
                                    tracing::info!("📁 检测到输出路径 (Merger): {}", path);
                                    output_file = Some(std::path::PathBuf::from(path));
                                }
                            }
                        }
                    }
                    
                    if line.contains("has already been downloaded") {
                        if let Some(start) = line.find("[download]") {
                            let rest = &line[start + 10..].trim();
                            if let Some(end) = rest.find(" has already been downloaded") {
                                let path = &rest[..end];
                                tracing::info!("📁 检测到输出路径 (already downloaded): {}", path);
                                output_file = Some(std::path::PathBuf::from(path));
                            }
                        }
                    }
                }
            }
            
            let _stderr_lines = stderr_handle.join().unwrap_or_default();
            
            // 如果没找到输出文件，尝试从输出中查找
            if output_file.is_none() {
                for line in &all_lines {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() 
                        && !trimmed.starts_with('[') 
                        && !trimmed.contains('%')
                        && (trimmed.contains('/') || trimmed.contains('.'))
                    {
                        let path = std::path::PathBuf::from(trimmed);
                        if path.exists() {
                            tracing::info!("📁 从输出检测到文件路径: {}", trimmed);
                            output_file = Some(path);
                            break;
                        }
                    }
                }
            }
            
            // 等待进程完成
            let status = child.wait()
                .map_err(|e| anyhow::anyhow!("等待 yt-dlp 完成失败: {}", e))?;
            
            if !status.success() {
                let error_context: Vec<String> = all_lines.iter()
                    .filter(|l| l.contains("ERROR") || l.contains("error") || l.contains("警告") || l.contains("Warning"))
                    .map(|s| s.clone())
                    .collect();
                let error_msg = if error_context.is_empty() {
                    let last_lines: Vec<String> = all_lines.iter().rev().take(5).map(|s| s.clone()).collect::<Vec<_>>().into_iter().rev().collect();
                    format!("下载失败，退出码: {:?}\n最后输出:\n{}", status.code(), last_lines.join("\n"))
                } else {
                    format!("下载失败，退出码: {:?}\n错误信息:\n{}", status.code(), error_context.join("\n"))
                };
                tracing::error!("❌ {}", error_msg);
                return Err(anyhow::anyhow!("{}", error_msg));
            }
            
            // 如果仍然没有找到输出文件，尝试在输出目录中查找最新文件
            if output_file.is_none() {
                tracing::warn!("⚠️ 未从 yt-dlp 输出中检测到文件路径，尝试在输出目录查找最新文件...");
                
                if let Ok(entries) = std::fs::read_dir(&output_dir) {
                    let mut newest_file: Option<(std::path::PathBuf, std::time::SystemTime)> = None;
                    for entry in entries.flatten() {
                        if let Ok(metadata) = entry.metadata() {
                            if metadata.is_file() {
                                if let Ok(modified) = metadata.modified() {
                                    if newest_file.is_none() || modified > newest_file.as_ref().unwrap().1 {
                                        newest_file = Some((entry.path(), modified));
                                    }
                                }
                            }
                        }
                    }
                    if let Some((path, _)) = newest_file {
                        tracing::info!("📁 找到最新文件: {:?}", path);
                        output_file = Some(path);
                    }
                }
            }
            
            match output_file {
                Some(path) => {
                    tracing::info!("✅ 下载完成: {:?}", path);
                    Ok(path)
                }
                None => {
                    let msg = format!("无法确定输出文件路径\n输出目录: {:?}\nyt-dlp 输出共 {} 行", 
                        output_dir, all_lines.len());
                    tracing::error!("❌ {}", msg);
                    Err(anyhow::anyhow!("{}", msg))
                }
            }
        })
    }
}

//! 应用程序状态管理
//!
//! 负责管理整个应用的状态，包括工具管理器实例、任务列表、配置等。

use anyhow::Result;
use gpui::Global;
use magekit_shared::{AppConfig, TaskStatus, TaskUpdate, DownloadOptions, TaskId};
use magekit_shared::{load_app_config_or_default, save_app_config};
use magekit_tool_manager::ToolManager;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::runtime::Runtime;

/// Global 包装器，用于在 GPUI 上下文中共享 AppState
pub struct GlobalAppState(pub Arc<AppState>);

impl Global for GlobalAppState {}

/// 应用程序主状态
pub struct AppState {
    /// 工具管理器
    pub tool_manager: Arc<ToolManager>,
    /// 应用配置
    pub config: Arc<RwLock<AppConfig>>,
    /// 任务状态列表
    pub tasks: Arc<RwLock<HashMap<TaskId, TaskStatus>>>,
    /// 事件接收器
    pub event_rx: mpsc::Receiver<AppEvent>,
    /// 事件发送器
    pub event_tx: mpsc::Sender<AppEvent>,
    /// Tokio 运行时 (用于异步任务)
    pub runtime: Arc<Runtime>,
}

/// 应用程序事件
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// 任务更新事件
    TaskUpdate(TaskUpdate),
    /// 配置变更事件
    ConfigChanged(AppConfig),
    /// 工具更新事件
    ToolUpdate(magekit_tool_manager::UpdateInfo),
    /// 显示通知
    ShowNotification(NotificationMessage),
}

/// 通知消息
#[derive(Debug, Clone)]
pub struct NotificationMessage {
    pub title: String,
    pub message: String,
    pub notification_type: NotificationType,
}

/// 通知类型
#[derive(Debug, Clone)]
pub enum NotificationType {
    Info,
    Success,
    Warning,
    Error,
}

/// 工具状态
#[derive(Debug, Clone)]
pub enum ToolStatus {
    /// 未安装
    NotInstalled,
    /// 已安装 (version: 版本号, is_system: 是否是系统安装)
    Installed { version: Option<String>, is_system: bool },
}

/// 视频下载选项
#[derive(Debug, Clone, Default)]
pub struct DownloadVideoOptions {
    pub embed_metadata: bool,
    pub embed_thumbnail: bool,
    pub download_subtitles: bool,
    pub audio_only: bool,
}

/// 解析大小字符串 (如 "1.5MiB", "500KiB") 为字节数
fn parse_size_string(s: &str) -> u64 {
    let s = s.trim();
    if s.is_empty() || s == "N/A" || s == "~" {
        return 0;
    }
    
    // 移除可能的单位后缀
    let (num_part, multiplier) = if s.ends_with("GiB") || s.ends_with("GB") {
        (s.trim_end_matches("GiB").trim_end_matches("GB"), 1024 * 1024 * 1024)
    } else if s.ends_with("MiB") || s.ends_with("MB") {
        (s.trim_end_matches("MiB").trim_end_matches("MB"), 1024 * 1024)
    } else if s.ends_with("KiB") || s.ends_with("KB") {
        (s.trim_end_matches("KiB").trim_end_matches("KB"), 1024)
    } else if s.ends_with("B") {
        (s.trim_end_matches("B"), 1)
    } else if s.ends_with("/s") {
        // 速度格式，递归处理
        return parse_size_string(s.trim_end_matches("/s"));
    } else {
        (s, 1)
    };
    
    num_part.trim().parse::<f64>().unwrap_or(0.0) as u64 * multiplier
}

impl AppState {
    /// 创建新的应用状态 (同步版本)
    pub fn new_sync() -> Result<Self> {
        // 创建 Tokio 运行时
        let runtime = Runtime::new()
            .map_err(|e| anyhow::anyhow!("创建 Tokio 运行时失败: {}", e))?;
        let runtime = Arc::new(runtime);

        // 创建事件通道
        let (event_tx, event_rx) = mpsc::channel(1000);

        // 从文件加载配置（如果失败则使用默认配置）
        let config = load_app_config_or_default();
        tracing::info!("Loaded config: download_path={:?}", config.download.default_output_path);
        let config = Arc::new(RwLock::new(config));

        // 创建工具管理器
        let tool_manager = Arc::new(ToolManager::new_sync()?);

        // 初始化任务列表
        let tasks = Arc::new(RwLock::new(HashMap::new()));

        Ok(Self {
            tool_manager,
            config,
            tasks,
            event_rx,
            event_tx,
            runtime,
        })
    }

    /// 创建新的应用状态 (异步版本，保持兼容)
    pub async fn new() -> Result<Self> {
        Self::new_sync()
    }

    /// 获取当前配置
    pub fn config(&self) -> AppConfig {
        // 使用 blocking_read 在同步上下文中读取
        self.config.blocking_read().clone()
    }

    /// 更新配置
    pub async fn update_config(&self, new_config: AppConfig) -> Result<()> {
        let mut config = self.config.write().await;
        *config = new_config.clone();
        
        // 保存配置到文件
        if let Err(e) = save_app_config(&new_config) {
            tracing::error!("Failed to save config to file: {}", e);
        } else {
            tracing::info!("Config saved successfully");
        }
        
        // 发送配置变更事件
        let _ = self.event_tx.send(AppEvent::ConfigChanged(new_config)).await;
        
        Ok(())
    }

    /// 开始下载任务
    pub async fn start_download(&self, url: String, options: Option<DownloadOptions>) -> Result<TaskId> {
        let download_options = options.unwrap_or_default();

        // 使用默认输出路径
        let output_path = std::env::current_dir()
            .unwrap_or_default()
            .join("downloads");

        let _options_with_path = DownloadOptions {
            output_path,
            ..download_options
        };

        // 开始下载（简化版本）
        let task_id = uuid::Uuid::new_v4();

        tracing::info!("开始下载任务: {} - {}", task_id, url);

        // 创建任务状态
        let task_status = TaskStatus::new(task_id, url.clone(), Some("视频标题".to_string()));

        // 存储任务
        {
            let mut tasks = self.tasks.write().await;
            tasks.insert(task_id, task_status);
        }

        // 发送通知
        let _ = self.event_tx.send(AppEvent::ShowNotification(NotificationMessage {
            title: "下载开始".to_string(),
            message: format!("开始下载: {}", url),
            notification_type: NotificationType::Info,
        })).await;

        Ok(task_id)
    }

    /// 暂停下载任务
    pub async fn pause_download(&self, task_id: TaskId) -> Result<()> {
        self.tool_manager.pause_download(task_id).await?;
        let _ = self.event_tx.send(AppEvent::ShowNotification(NotificationMessage {
            title: "下载已暂停".to_string(),
            message: format!("任务 {} 已暂停", task_id),
            notification_type: NotificationType::Warning,
        })).await;
        Ok(())
    }

    /// 恢复下载任务
    pub async fn resume_download(&self, task_id: TaskId) -> Result<()> {
        self.tool_manager.resume_download(task_id).await?;
        let _ = self.event_tx.send(AppEvent::ShowNotification(NotificationMessage {
            title: "下载已恢复".to_string(),
            message: format!("任务 {} 已恢复", task_id),
            notification_type: NotificationType::Info,
        })).await;
        Ok(())
    }

    /// 取消下载任务
    pub async fn cancel_download(&self, task_id: TaskId) -> Result<()> {
        self.tool_manager.cancel_download(task_id).await?;

        // 从任务列表中移除
        let mut tasks = self.tasks.write().await;
        tasks.remove(&task_id);

        let _ = self.event_tx.send(AppEvent::ShowNotification(NotificationMessage {
            title: "下载已取消".to_string(),
            message: format!("任务 {} 已取消", task_id),
            notification_type: NotificationType::Warning,
        })).await;
        Ok(())
    }

    /// 获取所有任务状态
    pub async fn get_all_tasks(&self) -> Vec<TaskStatus> {
        self.tool_manager.get_all_tasks().await
    }

    /// 获取指定任务状态
    pub async fn get_task_status(&self, task_id: TaskId) -> Option<TaskStatus> {
        self.tool_manager.get_task_status(task_id).await
    }

    /// 发送事件
    pub async fn send_event(&self, event: AppEvent) -> Result<()> {
        self.event_tx.send(event).await
            .map_err(|e| anyhow::anyhow!("发送事件失败: {}", e))?;
        Ok(())
    }

    /// 检查工具是否已安装
    pub async fn is_tool_installed(&self, tool_type: magekit_shared::ToolType) -> bool {
        self.tool_manager.storage.is_tool_installed(tool_type).await
    }

    /// 获取工具版本
    pub async fn get_tool_version(&self, tool_type: magekit_shared::ToolType) -> Option<String> {
        self.tool_manager.storage.get_tool_version(tool_type).await.ok().flatten()
    }

    /// 检测工具状态 - 同步版本 (用于 UI 初始化)
    pub fn check_tool_status_sync(&self, tool_type: magekit_shared::ToolType) -> ToolStatus {
        // 先检查应用内安装 (使用同步版本)
        if self.tool_manager.storage.is_tool_installed_sync(tool_type) {
            let version = self.tool_manager.storage.get_tool_version_sync(tool_type).ok().flatten();
            return ToolStatus::Installed { version, is_system: false };
        }

        // 再检查系统 PATH
        let tool_name = match tool_type {
            magekit_shared::ToolType::YtDlp => "yt-dlp",
            magekit_shared::ToolType::Ffmpeg => "ffmpeg",
        };

        if let Ok(path) = which::which(tool_name) {
            // 获取系统工具版本
            let version = Self::get_system_tool_version_sync(tool_type, &path);
            return ToolStatus::Installed { version, is_system: true };
        }

        ToolStatus::NotInstalled
    }

    /// 检测工具状态 - 异步版本 (用于后台任务)
    /// 注意: 此方法虽然是 async，但内部使用同步调用，因为 GPUI 的 spawn 不在 Tokio 运行时中
    pub async fn check_tool_status(&self, tool_type: magekit_shared::ToolType) -> ToolStatus {
        self.check_tool_status_sync(tool_type)
    }

    /// 获取系统工具版本 (同步版本)
    fn get_system_tool_version_sync(tool_type: magekit_shared::ToolType, path: &std::path::Path) -> Option<String> {
        use std::process::Command;
        
        let output = Command::new(path)
            .arg("--version")
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let version_output = String::from_utf8_lossy(&output.stdout);

        match tool_type {
            magekit_shared::ToolType::YtDlp => {
                // yt-dlp 输出格式: "yt-dlp 2023.07.06" 或 "2023.07.06"
                version_output
                    .lines()
                    .next()
                    .and_then(|line| {
                        // 查找以 20 开头的版本号（如 2023.07.06）
                        line.split_whitespace()
                            .find(|s| s.starts_with("20"))
                    })
                    .map(|v| v.to_string())
            }
            magekit_shared::ToolType::Ffmpeg => {
                // ffmpeg 输出格式: "ffmpeg version 8.0 Copyright..." 或 "ffmpeg version 5.1.2"
                version_output
                    .lines()
                    .find(|line| line.contains("ffmpeg version"))
                    .and_then(|line| {
                        // 获取 "version" 后面的部分
                        line.split("version").nth(1)
                            .and_then(|rest| rest.split_whitespace().next())
                    })
                    .map(|v| v.to_string())
            }
        }
    }

    /// 安装单个工具 (使用内部 Tokio 运行时，在后台线程中运行)
    pub fn install_tool_in_background(&self, tool_type: magekit_shared::ToolType) -> std::thread::JoinHandle<Result<()>> {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();
        
        std::thread::spawn(move || {
            runtime.block_on(async {
                match tool_type {
                    magekit_shared::ToolType::YtDlp => {
                        let updater = magekit_tool_manager::updater::ToolUpdater::new(tool_manager.storage.clone());
                        updater.ensure_yt_dlp(magekit_shared::UpdateChannel::Stable).await
                            .map_err(|e| anyhow::anyhow!("安装 yt-dlp 失败: {}", e))?;
                    }
                    magekit_shared::ToolType::Ffmpeg => {
                        let updater = magekit_tool_manager::updater::ToolUpdater::new(tool_manager.storage.clone());
                        updater.ensure_ffmpeg().await
                            .map_err(|e| anyhow::anyhow!("安装 ffmpeg 失败: {}", e))?;
                    }
                }
                Ok(())
            })
        })
    }

    /// 安装单个工具 (带进度回调，在后台线程中运行)
    /// progress_callback: fn(downloaded, total, speed)
    pub fn install_tool_with_progress(
        &self,
        tool_type: magekit_shared::ToolType,
        progress_callback: std::sync::Arc<dyn Fn(u64, u64, u64) + Send + Sync>,
    ) -> std::thread::JoinHandle<Result<()>> {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();
        
        std::thread::spawn(move || {
            runtime.block_on(async {
                match tool_type {
                    magekit_shared::ToolType::YtDlp => {
                        let updater = magekit_tool_manager::updater::ToolUpdater::new(tool_manager.storage.clone());
                        updater.ensure_yt_dlp_with_progress(
                            magekit_shared::UpdateChannel::Stable,
                            Some(progress_callback),
                        ).await
                            .map_err(|e| anyhow::anyhow!("安装 yt-dlp 失败: {}", e))?;
                    }
                    magekit_shared::ToolType::Ffmpeg => {
                        // ffmpeg 暂不支持进度回调
                        let updater = magekit_tool_manager::updater::ToolUpdater::new(tool_manager.storage.clone());
                        updater.ensure_ffmpeg().await
                            .map_err(|e| anyhow::anyhow!("安装 ffmpeg 失败: {}", e))?;
                    }
                }
                Ok(())
            })
        })
    }

    /// 删除已安装的工具
    pub fn delete_tool_sync(&self, tool_type: magekit_shared::ToolType) -> Result<()> {
        self.tool_manager.storage.delete_tool_sync(tool_type)
            .map_err(|e| anyhow::anyhow!("删除工具失败: {}", e))
    }

    /// 获取视频信息 (在后台线程中运行，返回 JoinHandle)
    /// 这个方法会在后台调用 yt-dlp --dump-json 获取视频信息
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
        progress_callback: std::sync::Arc<dyn Fn(f32, u64, u64, u64) + Send + Sync>,
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
            
            // 构建命令
            let mut cmd = std::process::Command::new(&yt_dlp_path);
            cmd.arg(&url)
               .arg("--format").arg(&format_id)
               .arg("--output").arg(output_dir.join("%(title)s.%(ext)s"))
               .arg("--newline")  // 每行输出进度，便于解析
               .arg("--progress-template").arg("download:%(progress._percent_str)s %(progress._speed_str)s %(progress._downloaded_bytes_str)s %(progress._total_bytes_str)s");
            
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
            
            // 读取 stderr 来获取进度
            let stderr = child.stderr.take()
                .ok_or_else(|| anyhow::anyhow!("无法获取 stderr"))?;
            
            // 同时读取 stdout
            let stdout = child.stdout.take()
                .ok_or_else(|| anyhow::anyhow!("无法获取 stdout"))?;
            
            use std::io::BufRead;
            let stderr_reader = std::io::BufReader::new(stderr);
            let stdout_reader = std::io::BufReader::new(stdout);
            
            let mut output_file: Option<std::path::PathBuf> = None;
            let mut all_lines: Vec<String> = Vec::new();
            
            // 在单独线程中读取 stdout
            let stdout_handle = std::thread::spawn(move || {
                let mut stdout_lines: Vec<String> = Vec::new();
                for line in stdout_reader.lines() {
                    if let Ok(line) = line {
                        tracing::debug!("[yt-dlp stdout] {}", line);
                        stdout_lines.push(line);
                    }
                }
                stdout_lines
            });
            
            // 在主线程中读取 stderr
            for line in stderr_reader.lines() {
                if let Ok(line) = line {
                    tracing::debug!("[yt-dlp stderr] {}", line);
                    all_lines.push(line.clone());
                    
                    // 解析进度信息
                    if line.starts_with("download:") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 4 {
                            // 解析百分比
                            let percent_str = parts[0].strip_prefix("download:").unwrap_or("0%");
                            let percent: f32 = percent_str.trim_end_matches('%')
                                .parse()
                                .unwrap_or(0.0);
                            
                            // 解析速度
                            let speed = parse_size_string(parts[1]);
                            
                            // 解析已下载
                            let downloaded = parse_size_string(parts[2]);
                            
                            // 解析总大小
                            let total = parse_size_string(parts[3]);
                            
                            progress_callback(percent, speed, downloaded, total);
                        }
                    }
                    
                    // 检测输出文件 - 支持多种格式
                    // [download] Destination: /path/to/file.mp4
                    if line.contains("Destination:") {
                        if let Some(path) = line.split("Destination:").nth(1) {
                            let path = path.trim();
                            tracing::info!("📁 检测到输出路径 (Destination): {}", path);
                            output_file = Some(std::path::PathBuf::from(path));
                        }
                    }
                    
                    // [Merger] Merging formats into "/path/to/file.mp4"
                    // [ffmpeg] Merging formats into "/path/to/file.webm"
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
                    
                    // [download] /path/to/file.mp4 has already been downloaded
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
                    
                    // [ExtractAudio] Destination: /path/to/file.mp3
                    // 已经被上面的 "Destination:" 处理
                }
            }
            
            // 获取 stdout 线程的结果
            let stdout_lines = stdout_handle.join().unwrap_or_default();
            
            // 如果还没有找到输出文件，检查 stdout 中的输出
            // yt-dlp 的 --print 选项会输出到 stdout
            if output_file.is_none() {
                for line in &stdout_lines {
                    let trimmed = line.trim();
                    // 检查是否像是文件路径
                    if !trimmed.is_empty() 
                        && !trimmed.starts_with('[') 
                        && !trimmed.contains('%')
                        && (trimmed.contains('/') || trimmed.contains('.'))
                    {
                        let path = std::path::PathBuf::from(trimmed);
                        // 检查文件是否存在
                        if path.exists() {
                            tracing::info!("📁 从 stdout 检测到输出路径: {}", trimmed);
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
                // 收集最后几行错误信息
                let error_context: Vec<String> = all_lines.iter()
                    .filter(|l| l.contains("ERROR") || l.contains("error") || l.contains("警告") || l.contains("Warning"))
                    .map(|s| s.clone())
                    .collect();
                let error_msg = if error_context.is_empty() {
                    let last_lines: Vec<String> = all_lines.iter().rev().take(5).map(|s| s.clone()).collect::<Vec<_>>().into_iter().rev().collect();
                    format!("下载失败，退出码: {:?}\n最后输出:\n{}", 
                        status.code(),
                        last_lines.join("\n"))
                } else {
                    format!("下载失败，退出码: {:?}\n错误信息:\n{}", 
                        status.code(),
                        error_context.join("\n"))
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
            
            // 返回输出文件路径
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

    /// 安装工具 (使用内部 Tokio 运行时) - 阻塞版本，不推荐使用
    #[allow(dead_code)]
    pub fn install_tool_blocking(&self, tool_type: magekit_shared::ToolType) -> Result<()> {
        use magekit_shared::UpdateChannel;
        use magekit_tool_manager::updater::ToolUpdater;
        
        self.runtime.block_on(async {
            let updater = ToolUpdater::new(self.tool_manager.storage.clone());
            match tool_type {
                magekit_shared::ToolType::YtDlp => {
                    updater.ensure_yt_dlp(UpdateChannel::Stable).await
                        .map_err(|e| anyhow::anyhow!("安装 yt-dlp 失败: {}", e))?;
                }
                magekit_shared::ToolType::Ffmpeg => {
                    updater.ensure_ffmpeg().await
                        .map_err(|e| anyhow::anyhow!("安装 ffmpeg 失败: {}", e))?;
                }
            }
            Ok(())
        })
    }

    /// 安装工具 (异步版本 - 在 Tokio 运行时中调用)
    pub async fn install_tool(&self, tool_type: magekit_shared::ToolType) -> Result<()> {
        use magekit_shared::UpdateChannel;
        
        match tool_type {
            magekit_shared::ToolType::YtDlp => {
                self.tool_manager.ensure_tools(UpdateChannel::Stable).await
                    .map_err(|e| anyhow::anyhow!("安装 yt-dlp 失败: {}", e))?;
            }
            magekit_shared::ToolType::Ffmpeg => {
                self.tool_manager.ensure_tools(UpdateChannel::Stable).await
                    .map_err(|e| anyhow::anyhow!("安装 ffmpeg 失败: {}", e))?;
            }
        }
        Ok(())
    }

    /// 安装所有工具 (使用内部 Tokio 运行时)
    pub fn install_all_tools_blocking(&self) -> Result<()> {
        use magekit_shared::UpdateChannel;
        self.runtime.block_on(async {
            self.tool_manager.ensure_tools(UpdateChannel::Stable).await
                .map_err(|e| anyhow::anyhow!("安装工具失败: {}", e))
        })
    }

    /// 安装所有工具 (后台线程，非阻塞)
    pub fn install_all_tools_in_background(&self) -> std::thread::JoinHandle<Result<()>> {
        let runtime = self.runtime.clone();
        let tool_manager = self.tool_manager.clone();
        
        std::thread::spawn(move || {
            runtime.block_on(async {
                tool_manager.ensure_tools(magekit_shared::UpdateChannel::Stable).await
                    .map_err(|e| anyhow::anyhow!("安装工具失败: {}", e))
            })
        })
    }

    /// 安装所有工具 (异步版本)
    pub async fn install_all_tools(&self) -> Result<()> {
        use magekit_shared::UpdateChannel;
        self.tool_manager.ensure_tools(UpdateChannel::Stable).await
            .map_err(|e| anyhow::anyhow!("安装工具失败: {}", e))?;
        Ok(())
    }

    /// 接收事件
    pub async fn recv_event(&mut self) -> Option<AppEvent> {
        self.event_rx.recv().await
    }

    /// 更新任务状态
    pub async fn update_task(&self, task_update: TaskUpdate) {
        match task_update {
            TaskUpdate::Created(task_status) => {
                let mut tasks = self.tasks.write().await;
                tasks.insert(task_status.id, task_status);
            }
            TaskUpdate::Progress(task_id, progress, downloaded, total, speed, eta) => {
                let mut tasks = self.tasks.write().await;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.progress = progress;
                    task.downloaded_bytes = downloaded;
                    task.total_bytes = total;
                    task.speed = speed;
                    task.eta = eta;
                }
            }
            TaskUpdate::StateChanged(task_id, state) => {
                let mut tasks = self.tasks.write().await;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.state = state.clone();

                    // 发送状态变更通知
                    let _notification_type = match state {
                        magekit_shared::TaskState::Completed => NotificationType::Success,
                        magekit_shared::TaskState::Failed(_) => NotificationType::Error,
                        _ => NotificationType::Info,
                    };

                    let _message = match state {
                        magekit_shared::TaskState::Completed => "下载完成".to_string(),
                        magekit_shared::TaskState::Failed(ref error) => format!("下载失败: {}", error),
                        _ => format!("任务状态变更为: {:?}", state),
                    };

                    // 这里应该发送通知，但由于借用检查器问题，暂时跳过
                    // let _ = self.event_tx.send(AppEvent::ShowNotification(NotificationMessage {
                    //     title: "任务状态更新".to_string(),
                    //     message,
                    //     notification_type,
                    // })).await;
                }
            }
            TaskUpdate::SpeedUpdate(task_id, speed) => {
                let mut tasks = self.tasks.write().await;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.speed = Some(speed);
                }
            }
            TaskUpdate::Completed(task_id, output_path) => {
                let mut tasks = self.tasks.write().await;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.state = magekit_shared::TaskState::Completed;
                    task.progress = 100.0;
                    task.output_path = Some(output_path);
                    task.completed_at = Some(std::time::SystemTime::now());
                }
            }
            TaskUpdate::Failed(task_id, error) => {
                let mut tasks = self.tasks.write().await;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.state = magekit_shared::TaskState::Failed(error.clone());
                    task.completed_at = Some(std::time::SystemTime::now());
                }
            }
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        // 使用 new_sync 来创建默认实例
        // 如果创建失败，程序将 panic (这是合理的，因为没有工具管理器应用无法运行)
        Self::new_sync().expect("Failed to create default AppState")
    }
}
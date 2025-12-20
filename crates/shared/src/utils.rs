//! 通用工具函数集合。

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

use crate::constants::*;

#[cfg(feature = "tools")]
use std::process::Command;

/// 创建一个在 Windows 上不显示控制台窗口的 Command (std::process::Command)
/// 在非 Windows 平台上，这只是普通的 Command::new
#[cfg(feature = "tools")]
#[cfg(windows)]
pub fn create_command<S: AsRef<std::ffi::OsStr>>(program: S) -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut cmd = Command::new(program);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(feature = "tools")]
#[cfg(not(windows))]
pub fn create_command<S: AsRef<std::ffi::OsStr>>(program: S) -> Command {
    Command::new(program)
}

/// 创建一个在 Windows 上不显示控制台窗口的 tokio Command
/// 在非 Windows 平台上，这只是普通的 tokio::process::Command::new
#[cfg(feature = "tools")]
#[cfg(windows)]
pub fn create_tokio_command<S: AsRef<std::ffi::OsStr>>(program: S) -> tokio::process::Command {
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut cmd = tokio::process::Command::new(program);
    // tokio::process::Command 在 Windows 上继承了 std::process::Command 的 creation_flags 方法
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(feature = "tools")]
#[cfg(not(windows))]
pub fn create_tokio_command<S: AsRef<std::ffi::OsStr>>(program: S) -> tokio::process::Command {
    tokio::process::Command::new(program)
}

/// 格式化文件大小为人类可读的字符串
pub fn format_file_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// 格式化速度为人类可读的字符串
pub fn format_speed(bytes_per_second: u64) -> String {
    format!("{}/s", format_file_size(bytes_per_second))
}

/// 格式化时间为人类可读的字符串
pub fn format_duration(duration: std::time::Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}", minutes, seconds)
    }
}

/// 验证 URL 是否有效且受支持
pub fn validate_url(url_str: &str) -> Result<Url> {
    if url_str.len() > url_validation::MAX_URL_LENGTH {
        anyhow::bail!("URL too long");
    }

    let url = Url::parse(url_str).context("Invalid URL format")?;

    // 检查协议
    if !url_validation::SUPPORTED_PROTOCOLS.contains(&url.scheme()) {
        anyhow::bail!("Unsupported URL protocol: {}", url.scheme());
    }

    // 检查域名
    if let Some(host) = url.host_str() {
        let is_supported = SUPPORTED_DOMAINS
            .iter()
            .any(|domain| host == *domain || host.ends_with(&format!(".{}", domain)));

        if !is_supported {
            // 不直接拒绝，只是给出警告
            tracing::warn!("URL domain may not be supported: {}", host);
        }
    }

    Ok(url)
}

/// 规范化 URL，确保有协议前缀
///
/// 处理常见的 URL 输入错误，如：
/// - `bilibili.com/video/...` -> `https://bilibili.com/video/...`
/// - `www.youtube.com/watch?v=...` -> `https://www.youtube.com/watch?v=...`
pub fn normalize_url(url: &str) -> String {
    let url_trimmed = url.trim();

    // 如果已经有协议，直接返回
    if url_trimmed.starts_with("http://") || url_trimmed.starts_with("https://") {
        return url_trimmed.to_string();
    }

    // 如果没有协议，添加 https://
    format!("https://{}", url_trimmed)
}

/// 获取应用的配置目录
pub fn get_app_config_dir() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .context("Failed to get config directory")?
        .join(APP_NAME);

    std::fs::create_dir_all(&config_dir).context("Failed to create config directory")?;

    Ok(config_dir)
}

/// 获取应用的数据目录
pub fn get_app_data_dir() -> Result<PathBuf> {
    let data_dir = dirs::data_dir()
        .context("Failed to get data directory")?
        .join(APP_NAME);

    std::fs::create_dir_all(&data_dir).context("Failed to create data directory")?;

    Ok(data_dir)
}

/// 获取工具目录
pub fn get_tools_dir() -> Result<PathBuf> {
    let data_dir = get_app_data_dir()?;
    let tools_dir = data_dir.join(paths::TOOLS_DIR_NAME);

    std::fs::create_dir_all(&tools_dir).context("Failed to create tools directory")?;

    Ok(tools_dir)
}

/// 解析 yt-dlp 可执行路径：优先应用内工具目录，其次系统 PATH
#[cfg(feature = "tools")]
pub fn resolve_yt_dlp_path() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    };
    if let Ok(dir) = get_tools_dir() {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    which::which("yt-dlp").ok()
}

/// 解析 ffmpeg 可执行路径：优先应用内工具目录，其次系统 PATH
#[cfg(feature = "tools")]
pub fn resolve_ffmpeg_path() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    if let Ok(dir) = get_tools_dir() {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    which::which("ffmpeg").ok()
}

/// 解析本机浏览器路径：优先用户自定义，其次常见安装位置/PATH
#[cfg(feature = "tools")]
pub fn resolve_browser_path(custom: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(path) = custom {
        if path.exists() {
            return Some(path);
        }
    }

    if cfg!(target_os = "windows") {
        for path in candidate_windows_browsers() {
            if path.exists() {
                return Some(path);
            }
        }
    } else if cfg!(target_os = "macos") {
        for path in [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ] {
            let p = PathBuf::from(path);
            if p.exists() {
                return Some(p);
            }
        }
    } else {
        for name in [
            "chrome",
            "google-chrome",
            "chromium",
            "chromium-browser",
            "edge",
        ] {
            if let Ok(p) = which::which(name) {
                return Some(p);
            }
        }
    }

    None
}

#[cfg(feature = "tools")]
fn candidate_windows_browsers() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let base_program_files =
        std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".into());
    let base_program_files_x86 =
        std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| "C:\\Program Files (x86)".into());

    let chrome = PathBuf::from(&base_program_files).join("Google/Chrome/Application/chrome.exe");
    let chrome_x86 =
        PathBuf::from(&base_program_files_x86).join("Google/Chrome/Application/chrome.exe");
    let edge = PathBuf::from(&base_program_files).join("Microsoft/Edge/Application/msedge.exe");
    let edge_x86 =
        PathBuf::from(&base_program_files_x86).join("Microsoft/Edge/Application/msedge.exe");

    candidates.push(chrome);
    candidates.push(chrome_x86);
    candidates.push(edge);
    candidates.push(edge_x86);

    candidates
}

/// 获取临时目录
pub fn get_temp_dir() -> Result<PathBuf> {
    let temp_dir = std::env::temp_dir().join(APP_NAME);

    std::fs::create_dir_all(&temp_dir).context("Failed to create temp directory")?;

    Ok(temp_dir)
}

/// 获取日志目录
pub fn get_log_dir() -> Result<PathBuf> {
    let data_dir = get_app_data_dir()?;
    let log_dir = data_dir.join(paths::LOG_DIR_NAME);

    std::fs::create_dir_all(&log_dir).context("Failed to create log directory")?;

    Ok(log_dir)
}

/// 生成安全的文件名
pub fn sanitize_filename(filename: &str) -> String {
    let invalid_chars = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    let mut result = String::new();

    for c in filename.chars() {
        if invalid_chars.contains(&c) {
            result.push('_');
        } else if c.is_control() {
            // 跳过控制字符
            continue;
        } else {
            result.push(c);
        }
    }

    // 移除开头和结尾的空格和点
    let result = result.trim_matches([' ', '.']).trim();

    // 如果结果为空，使用默认名称
    if result.is_empty() {
        "untitled".to_string()
    } else {
        result.to_string()
    }
}

/// 获取当前时间戳（秒）
pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// 生成唯一的输出文件路径
pub fn generate_output_path(base_dir: &Path, title: &str, extension: &str) -> Result<PathBuf> {
    let safe_title = sanitize_filename(title);
    let mut path = base_dir.join(format!("{}.{}", safe_title, extension));

    // 如果文件已存在，添加数字后缀
    let mut counter = 1;
    while path.exists() {
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_str()
            .unwrap_or("untitled");
        path = base_dir.join(format!("{}_{}.{}", stem, counter, extension));
        counter += 1;
    }

    Ok(path)
}

/// 解析文件大小字符串（如 "10MB"）为字节数
pub fn parse_file_size(size_str: &str) -> Result<u64> {
    let size_str = size_str.trim().to_uppercase();
    let (num_str, unit) = size_str.split_at(
        size_str
            .find(|c: char| !c.is_ascii_digit() && c != '.' && c != ' ')
            .unwrap_or(size_str.len()),
    );

    let num: f64 = num_str
        .trim()
        .parse()
        .context("Invalid number in file size")?;

    let multiplier = match unit.trim() {
        "B" | "" => 1.0,
        "KB" => file_size::KB as f64,
        "MB" => file_size::MB as f64,
        "GB" => file_size::GB as f64,
        "TB" => file_size::TB as f64,
        _ => anyhow::bail!("Unknown file size unit: {}", unit),
    };

    Ok((num * multiplier) as u64)
}

/// 检查路径是否可写
pub fn is_path_writable(path: &Path) -> bool {
    if path.exists() {
        // 如果路径存在，检查是否可写
        path.metadata()
            .map(|m| !m.permissions().readonly())
            .unwrap_or(false)
    } else {
        // 如果路径不存在，检查父目录是否可写
        path.parent().map(is_path_writable).unwrap_or(false)
    }
}

/// 获取可用的磁盘空间
pub fn get_available_space(path: &Path) -> Result<u64> {
    // 简化实现，返回一个合理的默认值
    // 在实际项目中，可以使用更复杂的平台特定实现
    std::fs::metadata(path).context("Failed to get path metadata")?;

    // 返回1GB的默认可用空间作为占位符
    Ok(1024 * 1024 * 1024)
}

/// 获取配置文件路径
pub fn get_config_file_path() -> Result<PathBuf> {
    let config_dir = get_app_config_dir()?;
    Ok(config_dir.join(CONFIG_FILE_NAME))
}

/// 保存应用配置到文件
pub fn save_app_config(config: &crate::types::AppConfig) -> Result<()> {
    let config_path = get_config_file_path()?;

    let toml_string =
        toml::to_string_pretty(config).context("Failed to serialize config to TOML")?;

    std::fs::write(&config_path, toml_string).context("Failed to write config file")?;

    tracing::info!("Configuration saved to: {:?}", config_path);
    Ok(())
}

/// 从文件加载应用配置
pub fn load_app_config() -> Result<crate::types::AppConfig> {
    let config_path = get_config_file_path()?;

    if !config_path.exists() {
        tracing::info!("Config file not found, using defaults: {:?}", config_path);
        return Ok(crate::types::AppConfig::default());
    }

    let content = std::fs::read_to_string(&config_path).context("Failed to read config file")?;

    let config: crate::types::AppConfig =
        toml::from_str(&content).context("Failed to parse config file")?;

    tracing::info!("Configuration loaded from: {:?}", config_path);
    Ok(config)
}

/// 加载应用配置，如果不存在则创建默认配置文件
pub fn load_app_config_or_default() -> crate::types::AppConfig {
    let config_path = match get_config_file_path() {
        Ok(path) => path,
        Err(e) => {
            tracing::warn!("Failed to get config file path: {}", e);
            return crate::types::AppConfig::default();
        }
    };

    if config_path.exists() {
        // 配置文件存在，尝试加载
        match load_app_config() {
            Ok(config) => config,
            Err(e) => {
                tracing::warn!("Failed to load config, using defaults: {}", e);
                crate::types::AppConfig::default()
            }
        }
    } else {
        // 配置文件不存在，创建默认配置
        let default_config = crate::types::AppConfig::default();
        if let Err(e) = save_app_config(&default_config) {
            tracing::warn!("Failed to save default config: {}", e);
        }
        default_config
    }
}

/// 安全截断字符串，确保在字符边界处截断
///
/// 避免 GPUI DirectWrite 在 Windows 上的 UTF-8 边界 bug。
/// 当使用 `text_ellipsis()` 或 `line_clamp()` 时，GPUI 可能在多字节字符
/// （如中文）的中间截断，导致 panic。
///
/// # 参数
/// - `s`: 要截断的字符串
/// - `max_chars`: 最大字符数（不是字节数）
///
/// # 返回
/// 截断后的字符串，如果被截断则末尾添加省略号 `…`
pub fn truncate_string(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(0), "0 B");
        assert_eq!(format_file_size(1024), "1.0 KB");
        assert_eq!(format_file_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_file_size(1536), "1.5 KB");
    }

    #[test]
    fn test_validate_url() {
        assert!(validate_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ").is_ok());
        assert!(validate_url("ftp://example.com/file").is_err());
        assert!(validate_url("not a url").is_err());
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("test/file.txt"), "test_file.txt");
        assert_eq!(sanitize_filename("test:file.txt"), "test_file.txt");
        assert_eq!(sanitize_filename(""), "untitled");
        assert_eq!(sanitize_filename("   "), "untitled");
    }

    #[test]
    fn test_parse_file_size() {
        assert_eq!(parse_file_size("1024").unwrap(), 1024);
        assert_eq!(parse_file_size("1KB").unwrap(), 1024);
        assert_eq!(
            parse_file_size("1.5MB").unwrap(),
            (1.5 * 1024.0 * 1024.0) as u64
        );
        assert!(parse_file_size("invalid").is_err());
    }
}

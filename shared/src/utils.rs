use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

use crate::constants::*;

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
        let is_supported = SUPPORTED_DOMAINS.iter().any(|domain| {
            host == *domain || host.ends_with(&format!(".{}", domain))
        });

        if !is_supported {
            // 不直接拒绝，只是给出警告
            tracing::warn!("URL domain may not be supported: {}", host);
        }
    }

    Ok(url)
}

/// 获取应用的配置目录
pub fn get_app_config_dir() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .context("Failed to get config directory")?
        .join(APP_NAME);

    std::fs::create_dir_all(&config_dir)
        .context("Failed to create config directory")?;

    Ok(config_dir)
}

/// 获取应用的数据目录
pub fn get_app_data_dir() -> Result<PathBuf> {
    let data_dir = dirs::data_dir()
        .context("Failed to get data directory")?
        .join(APP_NAME);

    std::fs::create_dir_all(&data_dir)
        .context("Failed to create data directory")?;

    Ok(data_dir)
}

/// 获取工具目录
pub fn get_tools_dir() -> Result<PathBuf> {
    let data_dir = get_app_data_dir()?;
    let tools_dir = data_dir.join(paths::TOOLS_DIR_NAME);

    std::fs::create_dir_all(&tools_dir)
        .context("Failed to create tools directory")?;

    Ok(tools_dir)
}

/// 获取临时目录
pub fn get_temp_dir() -> Result<PathBuf> {
    let temp_dir = std::env::temp_dir().join(APP_NAME);

    std::fs::create_dir_all(&temp_dir)
        .context("Failed to create temp directory")?;

    Ok(temp_dir)
}

/// 获取日志目录
pub fn get_log_dir() -> Result<PathBuf> {
    let data_dir = get_app_data_dir()?;
    let log_dir = data_dir.join(paths::LOG_DIR_NAME);

    std::fs::create_dir_all(&log_dir)
        .context("Failed to create log directory")?;

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
pub fn generate_output_path(
    base_dir: &Path,
    title: &str,
    extension: &str,
) -> Result<PathBuf> {
    let safe_title = sanitize_filename(title);
    let mut path = base_dir.join(format!("{}.{}", safe_title, extension));

    // 如果文件已存在，添加数字后缀
    let mut counter = 1;
    while path.exists() {
        let stem = path.file_stem().unwrap_or_default().to_str().unwrap_or("untitled");
        path = base_dir.join(format!("{}_{}.{}", stem, counter, extension));
        counter += 1;
    }

    Ok(path)
}

/// 解析文件大小字符串（如 "10MB"）为字节数
pub fn parse_file_size(size_str: &str) -> Result<u64> {
    let size_str = size_str.trim().to_uppercase();
    let (num_str, unit) = size_str.split_at(
        size_str.find(|c: char| !c.is_ascii_digit() && c != ' ').unwrap_or(size_str.len())
    );

    let num: f64 = num_str.trim().parse()
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
        path.metadata().map(|m| !m.permissions().readonly()).unwrap_or(false)
    } else {
        // 如果路径不存在，检查父目录是否可写
        path.parent().map(is_path_writable).unwrap_or(false)
    }
}

/// 获取可用的磁盘空间
pub fn get_available_space(path: &Path) -> Result<u64> {
    // 简化实现，返回一个合理的默认值
    // 在实际项目中，可以使用更复杂的平台特定实现
    std::fs::metadata(path)
        .context("Failed to get path metadata")?;

    // 返回1GB的默认可用空间作为占位符
    Ok(1024 * 1024 * 1024)
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
        assert_eq!(parse_file_size("1.5MB").unwrap(), 1.5 * 1024.0 * 1024.0 as u64);
        assert!(parse_file_size("invalid").is_err());
    }
}
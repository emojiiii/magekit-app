//! 工具函数

/// 解析大小字符串 (如 "1.5MiB", "500KiB") 为字节数
pub fn parse_size_string(s: &str) -> u64 {
    let s = s.trim();
    if s.is_empty() || s == "N/A" || s == "~" || s == "Unknown" {
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

/// 从 yt-dlp 输出行中提取百分比
/// 示例: "[download]  45.2% of ~12.34MiB at 1.23MiB/s ETA 00:05"
pub fn extract_percent(line: &str) -> f32 {
    if let Some(percent_pos) = line.find('%') {
        let before_percent = &line[..percent_pos];
        if let Some(num_str) = before_percent.split_whitespace().last() {
            return num_str.parse().unwrap_or(0.0);
        }
    }
    0.0
}

/// 从 yt-dlp 输出行中提取总大小
/// 示例: "[download]  45.2% of ~12.34MiB at 1.23MiB/s ETA 00:05"
pub fn extract_total_size(line: &str) -> u64 {
    if let Some(of_pos) = line.find(" of ") {
        let after_of = &line[of_pos + 4..];
        // 取第一个空格前的部分
        if let Some(size_str) = after_of.split_whitespace().next() {
            // 移除可能的 ~ 前缀
            let size_str = size_str.trim_start_matches('~');
            return parse_size_string(size_str);
        }
    }
    0
}

/// 从 yt-dlp 输出行中提取下载速度
/// 示例: "[download]  45.2% of ~12.34MiB at 1.23MiB/s ETA 00:05"
pub fn extract_speed(line: &str) -> u64 {
    if let Some(at_pos) = line.find(" at ") {
        let after_at = &line[at_pos + 4..];
        if let Some(speed_str) = after_at.split_whitespace().next() {
            return parse_size_string(speed_str);
        }
    }
    0
}

/// 生成唯一的文件路径，如果文件已存在则添加序号 (1), (2) 等
/// 类似浏览器的下载行为
#[allow(dead_code)]
pub fn get_unique_file_path(base_path: &std::path::Path) -> std::path::PathBuf {
    if !base_path.exists() {
        return base_path.to_path_buf();
    }
    
    let parent = base_path.parent().unwrap_or(std::path::Path::new("."));
    let stem = base_path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let extension = base_path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    
    let mut index = 1;
    loop {
        let new_name = if extension.is_empty() {
            format!("{} ({})", stem, index)
        } else {
            format!("{} ({}).{}", stem, index, extension)
        };
        let new_path = parent.join(&new_name);
        if !new_path.exists() {
            return new_path;
        }
        index += 1;
        // 防止无限循环
        if index > 1000 {
            // 使用时间戳作为后备
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let new_name = if extension.is_empty() {
                format!("{}_{}", stem, timestamp)
            } else {
                format!("{}_{}.{}", stem, timestamp, extension)
            };
            return parent.join(&new_name);
        }
    }
}

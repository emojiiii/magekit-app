use std::path::PathBuf;
use std::{collections::hash_map::DefaultHasher, hash::Hasher, path::Path};

use crate::config::DownloadRequest;
use crate::error::DownloadResult;

/// 根据请求与 URL 生成输出路径
///
/// 优先级：
/// 1. full_path（如果指定）- 直接使用，不做任何修改
/// 2. directory + template - 旧逻辑，支持 %(ext)s 占位符
pub fn resolve_output_path(request: &DownloadRequest, url: &url::Url) -> DownloadResult<PathBuf> {
    // 🔧 优先使用 full_path
    if let Some(full_path) = &request.output.full_path {
        tracing::debug!("📁 使用指定的完整路径: {:?}", full_path);
        return Ok(full_path.clone());
    }

    // 降级到 directory + template 逻辑
    let dir = request.output.directory.clone();
    let tmpl = request
        .output
        .template
        .clone()
        .unwrap_or_else(|| "download.bin".to_string());

    let ext_from_path = url
        .path_segments()
        .and_then(|mut segs| segs.next_back())
        .and_then(|s| {
            // 去除查询参数
            let s_without_query = s.split('?').next().unwrap_or(s);
            s_without_query.rsplit_once('.').map(|(_, ext)| ext)
        })
        .filter(|s| !s.is_empty())
        .map(|ext| {
            // 🔧 流媒体格式转换为mp4（向后兼容）
            let ext_lower = ext.to_lowercase();
            if ext_lower == "m3u8" || ext_lower == "m3u" || ext_lower == "mpd" || ext_lower == "ts"
            {
                "mp4"
            } else {
                ext
            }
        });

    let filename = if tmpl.contains("%(ext)s") {
        if let Some(ext) = ext_from_path {
            tmpl.replace("%(ext)s", ext)
        } else {
            tmpl.replace("%(ext)s", "bin")
        }
    } else {
        tmpl
    };

    Ok(dir.join(filename))
}

/// 直链/合并临时文件路径（`<filename>.part`）
pub fn part_path_for_output(output_path: &Path) -> PathBuf {
    let file_name = output_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("download.bin");
    output_path.with_file_name(format!("{}.part", file_name))
}

/// HLS 分段缓存目录（稳定且尽量避免冲突）
///
/// 位置：`<output_dir>/.magekit-hls-cache/<hash>/`
/// hash 输入包含 url 与 output_path，确保不同输出/不同资源不会互相覆盖。
pub fn hls_cache_dir(output_path: &Path, url: &url::Url) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    hasher.write(url.as_str().as_bytes());
    hasher.write(output_path.to_string_lossy().as_bytes());
    let hash = hasher.finish();

    let output_dir = output_path.parent().unwrap_or_else(|| Path::new("."));
    output_dir
        .join(".magekit-hls-cache")
        .join(format!("{:016x}", hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DownloadOutput;

    #[test]
    fn test_resolve_output_path_m3u8_to_mp4() {
        let dir = std::path::PathBuf::from("/tmp");
        let output = DownloadOutput {
            directory: dir.clone(),
            template: Some("video.%(ext)s".to_string()),
            full_path: None,
        };

        let url = url::Url::parse("https://example.com/video.m3u8?auth=123").unwrap();
        let request = crate::config::DownloadRequest::new(url.clone(), output.clone());

        let path = resolve_output_path(&request, &url).unwrap();
        assert_eq!(path, dir.join("video.mp4"));
    }

    #[test]
    fn test_resolve_output_path_direct_template() {
        let dir = std::path::PathBuf::from("/tmp");
        let output = DownloadOutput {
            directory: dir.clone(),
            template: Some("my_video.mp4".to_string()),
            full_path: None,
        };

        let url = url::Url::parse("https://example.com/stream.m3u8").unwrap();
        let request = crate::config::DownloadRequest::new(url.clone(), output.clone());

        let path = resolve_output_path(&request, &url).unwrap();
        assert_eq!(path, dir.join("my_video.mp4"));
    }
}

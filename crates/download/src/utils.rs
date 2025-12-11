use std::path::PathBuf;

use crate::config::DownloadRequest;
use crate::error::DownloadResult;

/// 根据请求与 URL 生成输出路径，支持模板中的 %(ext)s
pub fn resolve_output_path(request: &DownloadRequest, url: &url::Url) -> DownloadResult<PathBuf> {
    let dir = request.output.directory.clone();
    let tmpl = request
        .output
        .template
        .clone()
        .unwrap_or_else(|| "download.bin".to_string());

    let ext_from_path = url
        .path_segments()
        .and_then(|mut segs| segs.next_back())
        .and_then(|s| s.rsplit_once('.').map(|(_, ext)| ext))
        .filter(|s| !s.is_empty());

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


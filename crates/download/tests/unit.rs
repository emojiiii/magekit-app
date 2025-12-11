use std::path::PathBuf;

use download::downloader::direct::parse_content_range_total;
use download::downloader::ffmpeg::{parse_ffmpeg_size, parse_ffmpeg_time, parse_timestamp_to_secs};
use download::downloader::ytdlp::{parse_destination, parse_progress_line};
use download::error::DownloadError;
use download::utils::resolve_output_path;
use url::Url;

#[test]
fn test_ytdlp_progress_parse() {
    let line = "[download]  45.2% of ~12.34MiB at 1.23MiB/s ETA 00:05";
    let (downloaded, total, speed) = parse_progress_line(line).unwrap();
    let total = total.unwrap();
    assert!((total as i64 - 12_940_032).abs() < 10_000);
    let downloaded = downloaded.unwrap();
    assert!(downloaded > 5_700_000 && downloaded < 6_000_000);
    let speed = speed.unwrap();
    assert!((speed as i64 - 1_289_000).abs() < 10_000);
}

#[test]
fn test_ytdlp_destination_parse() {
    let line = "[download] Destination: /tmp/video.mp4";
    let path = parse_destination(line).unwrap();
    assert_eq!(path, PathBuf::from("/tmp/video.mp4"));
}

#[test]
fn test_ytdlp_progress_partial_fields() {
    // 缺少 total/speed 时应解析为 None
    let line = "[download]  12.0% of ~ at  ETA 00:05";
    let (downloaded, total, speed) = parse_progress_line(line).unwrap();
    assert!(total.is_none());
    assert!(speed.is_none());
    assert!(downloaded.is_none());
}

#[test]
fn test_ytdlp_destination_none() {
    assert!(parse_destination("no destination here").is_none());
}

#[test]
fn test_ffmpeg_size_time_parse() {
    let line = "frame=   42 fps=0.0 q=-1.0 size=    123kB time=00:00:02.10 bitrate= 479.3kbits/s";
    let size = parse_ffmpeg_size(line).unwrap();
    assert!(size > 120_000 && size < 130_000);
    let time = parse_ffmpeg_time(line).unwrap();
    assert!((time - 2.10).abs() < 0.01);
}

#[test]
fn test_ffmpeg_size_missing() {
    let line = "frame=   42 fps=0.0 q=-1.0 time=00:00:02.10 bitrate= 479.3kbits/s";
    assert!(parse_ffmpeg_size(line).is_none());
}

#[test]
fn test_timestamp_parse() {
    let t = parse_timestamp_to_secs("01:02:03.50").unwrap();
    assert!((t - 3723.5).abs() < 1e-3);
}

#[test]
fn test_content_range_parse() {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_RANGE,
        "bytes 0-5/123".parse().unwrap(),
    );
    assert_eq!(parse_content_range_total(&headers), Some(123));

    let empty = reqwest::header::HeaderMap::new();
    assert!(parse_content_range_total(&empty).is_none());
}

#[test]
fn test_resolve_output_path_with_ext() {
    let url: Url = "https://example.com/path/video.mp4".parse().unwrap();
    let req = download::config::DownloadRequest::new(
        url.clone(),
        download::config::DownloadOutput {
            directory: std::path::PathBuf::from("/tmp"),
            template: Some("%(ext)s".into()),
        },
    );
    let path = resolve_output_path(&req, &url).unwrap();
    assert_eq!(path, std::path::PathBuf::from("/tmp/mp4"));
}

#[test]
fn test_resolve_output_path_default_bin() {
    let url: Url = "https://example.com/path/noext".parse().unwrap();
    let req = download::config::DownloadRequest::new(
        url.clone(),
        download::config::DownloadOutput {
            directory: std::path::PathBuf::from("/tmp"),
            template: Some("file.%(ext)s".into()),
        },
    );
    let path = resolve_output_path(&req, &url).unwrap();
    assert_eq!(path, std::path::PathBuf::from("/tmp/file.bin"));
}

#[test]
fn test_resolve_output_path_default_template() {
    let url: Url = "https://example.com/with.ext".parse().unwrap();
    let req = download::config::DownloadRequest::new(
        url.clone(),
        download::config::DownloadOutput {
            directory: std::path::PathBuf::from("/tmp"),
            template: None,
        },
    );
    let path = resolve_output_path(&req, &url).unwrap();
    // 默认模板 download.bin
    assert_eq!(path, std::path::PathBuf::from("/tmp/download.bin"));
}

#[test]
fn test_download_error_clone() {
    let e1 = DownloadError::Network("fail".into());
    let e2 = e1.clone();
    assert_eq!(format!("{e1}"), format!("{e2}"));
}

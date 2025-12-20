use std::convert::Infallible;
use std::net::SocketAddr;

use download::progress::DownloadCallback;
use download::{DownloadClient, DownloadOutput, DownloadRequest, DownloadStrategy};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

struct NoopCallback;
impl DownloadCallback for NoopCallback {}

#[tokio::test]
async fn direct_resume_range() {
    let (addr, shutdown_tx) = spawn_test_server().await;
    let base = format!("http://{}", addr);

    let dir = tempfile::tempdir().unwrap();
    let output = DownloadOutput {
        directory: dir.path().into(),
        template: Some("file.bin".into()),
        full_path: None,
    };
    let url = format!("{}/file.bin", base).parse().unwrap();
    let mut req = DownloadRequest::new(url, output);
    req.strategy = DownloadStrategy::Direct;
    req.resume = true;
    req.timeout = Some(std::time::Duration::from_secs(10));

    // 模拟中断：预写入部分 .part 文件
    let partial_path = dir.path().join("file.bin.part");
    tokio::fs::write(&partial_path, b"hello").await.unwrap();

    let client = DownloadClient::with_defaults();
    let cancel = CancellationToken::new();
    let outcome = client
        .download(req, &NoopCallback, cancel)
        .await
        .expect("download");

    let data = tokio::fs::read(&outcome.output_path).await.unwrap();
    assert_eq!(data, b"hello world!");

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn direct_no_range_fallback() {
    let (addr, shutdown_tx) = spawn_test_server().await;
    let base = format!("http://{}", addr);

    let dir = tempfile::tempdir().unwrap();
    let output = DownloadOutput {
        directory: dir.path().into(),
        template: Some("norange.bin".into()),
        full_path: None,
    };
    let url = format!("{}/norange.bin", base).parse().unwrap();
    let mut req = DownloadRequest::new(url, output);
    req.strategy = DownloadStrategy::Direct;
    req.resume = true;

    // 先写入部分 .part
    let partial_path = dir.path().join("norange.bin.part");
    tokio::fs::write(&partial_path, b"partial").await.unwrap();

    let client = DownloadClient::with_defaults();
    let cancel = CancellationToken::new();
    let outcome = client
        .download(req, &NoopCallback, cancel)
        .await
        .expect("download");

    let data = tokio::fs::read(&outcome.output_path).await.unwrap();
    assert_eq!(data, b"FULL");

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn hls_concurrent_download() {
    let (addr, shutdown_tx) = spawn_test_server().await;
    let base = format!("http://{}", addr);

    let dir = tempfile::tempdir().unwrap();
    let output = DownloadOutput {
        directory: dir.path().into(),
        template: Some("out.ts".into()),
        full_path: None,
    };
    let url = format!("{}/master.m3u8", base).parse().unwrap();
    let mut req = DownloadRequest::new(url, output);
    req.strategy = DownloadStrategy::HlsDash;
    req.timeout = Some(std::time::Duration::from_secs(10));

    let client = DownloadClient::with_defaults();
    let cancel = CancellationToken::new();
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        client.download(req, &NoopCallback, cancel),
    )
    .await
    .expect("hls timeout")
    .expect("hls download");

    let data = tokio::fs::read(&outcome.output_path).await.unwrap();
    assert_eq!(data, b"AAA\nBBB\n");

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn direct_headers_cookie() {
    let (addr, shutdown_tx) = spawn_test_server().await;
    let base = format!("http://{}", addr);

    let dir = tempfile::tempdir().unwrap();
    let output = DownloadOutput {
        directory: dir.path().into(),
        template: Some("echo.bin".into()),
        full_path: None,
    };
    let url = format!("{}/echo-headers", base).parse().unwrap();
    let mut req = DownloadRequest::new(url, output);
    req.strategy = DownloadStrategy::Direct;
    req.extra.headers.insert("X-Test".into(), "abc".into());
    req.extra.cookie = Some("c1=v1".into());

    let client = DownloadClient::with_defaults();
    let cancel = CancellationToken::new();
    let outcome = client
        .download(req, &NoopCallback, cancel)
        .await
        .expect("download");

    let data = tokio::fs::read(&outcome.output_path).await.unwrap();
    let body = String::from_utf8_lossy(&data);
    assert!(body.contains("X-Test: abc"));
    assert!(body.contains("Cookie: c1=v1"));

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn hls_aes_decrypt_and_order() {
    let (addr, shutdown_tx) = spawn_test_server().await;
    let base = format!("http://{}", addr);

    let dir = tempfile::tempdir().unwrap();
    let output = DownloadOutput {
        directory: dir.path().into(),
        template: Some("aes.ts".into()),
        full_path: None,
    };
    let url = format!("{}/media_aes.m3u8", base).parse().unwrap();
    let mut req = DownloadRequest::new(url, output);
    req.strategy = DownloadStrategy::HlsDash;
    req.timeout = Some(std::time::Duration::from_secs(10));

    let client = DownloadClient::with_defaults();
    let cancel = CancellationToken::new();
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        client.download(req, &NoopCallback, cancel),
    )
    .await
    .expect("hls aes timeout")
    .expect("hls aes download");

    let data = tokio::fs::read(&outcome.output_path).await.unwrap();
    assert_eq!(data, b"SEG0\nSEG1\n");

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn direct_should_fail_on_bad_host() {
    let dir = tempfile::tempdir().unwrap();
    let output = DownloadOutput {
        directory: dir.path().into(),
        template: Some("bad.bin".into()),
        full_path: None,
    };
    let url = "http://invalid.example.invalid/404.bin".parse().unwrap();
    let mut req = DownloadRequest::new(url, output);
    req.strategy = DownloadStrategy::Direct;
    req.timeout = Some(std::time::Duration::from_secs(2));

    let client = DownloadClient::with_defaults();
    let cancel = CancellationToken::new();
    let res = client.download(req, &NoopCallback, cancel).await;
    assert!(res.is_err());
}

#[tokio::test]
async fn direct_timeout() {
    let (addr, shutdown_tx) = spawn_test_server().await;
    let base = format!("http://{}", addr);

    let dir = tempfile::tempdir().unwrap();
    let output = DownloadOutput {
        directory: dir.path().into(),
        template: Some("slow.bin".into()),
        full_path: None,
    };
    let url = format!("{}/slow.bin", base).parse().unwrap();
    let mut req = DownloadRequest::new(url, output);
    req.strategy = DownloadStrategy::Direct;
    req.timeout = Some(std::time::Duration::from_millis(10));

    let client = DownloadClient::with_defaults();
    let cancel = CancellationToken::new();
    let res = client.download(req, &NoopCallback, cancel).await;
    assert!(res.is_err());

    // 超时失败后 .part 应不存在
    assert!(!dir.path().join("slow.bin.part").exists());

    let _ = shutdown_tx.send(());
}

async fn spawn_test_server() -> (SocketAddr, oneshot::Sender<()>) {
    let (tx, rx) = oneshot::channel();
    let make_svc =
        make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle_request)) });

    let server = Server::bind(&([127, 0, 0, 1], 0).into()).serve(make_svc);
    let addr = server.local_addr();
    let graceful = server.with_graceful_shutdown(async {
        let _ = rx.await;
    });
    tokio::spawn(graceful);
    (addr, tx)
}

async fn handle_request(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let path = req.uri().path();
    match path {
        "/file.bin" => {
            let full = b"hello world!";
            if let Some(range) = req.headers().get("range").and_then(|v| v.to_str().ok()) {
                if let Some(start) = range
                    .strip_prefix("bytes=")
                    .and_then(|s| s.trim_end_matches('-').parse::<usize>().ok())
                {
                    let slice = &full[start.min(full.len())..];
                    let body = Body::from(slice.to_vec());
                    let resp = Response::builder()
                        .status(StatusCode::PARTIAL_CONTENT)
                        .header(
                            "Content-Range",
                            format!("bytes {}-{}/{}", start, full.len() - 1, full.len()),
                        )
                        .body(body)
                        .unwrap();
                    return Ok(resp);
                }
            }
            Ok(Response::new(Body::from(full.to_vec())))
        }
        "/norange.bin" => {
            // 无论 Range 与否都返回 200 全量
            Ok(Response::new(Body::from("FULL")))
        }
        "/master.m3u8" => {
            let body = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1000\n/media.m3u8\n";
            Ok(Response::new(Body::from(body)))
        }
        "/media.m3u8" => {
            let body = "#EXTM3U\n#EXTINF:1,\n/s1.ts\n#EXTINF:1,\n/s2.ts\n#EXT-X-ENDLIST\n";
            Ok(Response::new(Body::from(body)))
        }
        "/media_aes.m3u8" => {
            let body = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-KEY:METHOD=AES-128,URI=\"/key.bin\",IV=0x00000000000000000000000000000001\n#EXTINF:2,\n/aes1.ts\n#EXTINF:2,\n/aes2.ts\n#EXT-X-ENDLIST\n";
            Ok(Response::new(Body::from(body)))
        }
        "/s1.ts" => Ok(Response::new(Body::from("AAA\n"))),
        "/s2.ts" => Ok(Response::new(Body::from("BBB\n"))),
        "/aes1.ts" => Ok(Response::new(Body::from(encrypt_segment(0)))),
        "/aes2.ts" => {
            // 故意延迟，验证乱序也能排序写入
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            Ok(Response::new(Body::from(encrypt_segment(1))))
        }
        "/key.bin" => Ok(Response::new(Body::from([1u8; 16].to_vec()))),
        "/slow.bin" => {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            Ok(Response::new(Body::from("slow")))
        }
        "/echo-headers" => {
            let mut lines = Vec::new();
            if let Some(v) = req.headers().get("x-test").and_then(|v| v.to_str().ok()) {
                lines.push(format!("X-Test: {}", v));
            }
            if let Some(v) = req.headers().get("cookie").and_then(|v| v.to_str().ok()) {
                lines.push(format!("Cookie: {}", v));
            }
            let body = lines.join("\n");
            Ok(Response::new(Body::from(body)))
        }
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap()),
    }
}

fn encrypt_segment(idx: u8) -> Vec<u8> {
    use aes::cipher::{BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
    type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;
    let key = [1u8; 16];
    let mut iv = [0u8; 16];
    iv[15] = 1; // 与 m3u8 的 IV 对齐
    let plaintext = format!("SEG{}\n", idx).into_bytes();
    Aes128CbcEnc::new(&key.into(), &iv.into()).encrypt_padded_vec_mut::<Pkcs7>(&plaintext)
}

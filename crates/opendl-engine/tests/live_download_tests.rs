use opendl_engine::{DownloadOptions, Downloader};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

async fn start_mock_range_server(payload: Vec<u8>) -> (SocketAddr, oneshot::Sender<()>) {
    let payload = Arc::new(payload);
    let total_len = payload.len();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                Ok((mut socket, _)) = listener.accept() => {
                    let payload_clone = payload.clone();
                    tokio::spawn(async move {
                        use tokio::io::{AsyncReadExt, AsyncWriteExt};
                        let mut buf = [0u8; 4096];
                        if let Ok(n) = socket.read(&mut buf).await {
                            if n == 0 { return; }
                            let req = String::from_utf8_lossy(&buf[..n]);

                            let is_head = req.starts_with("HEAD");
                            let range_header = req.lines()
                                .find(|l| l.to_lowercase().starts_with("range:"))
                                .and_then(|l| l.split(':').nth(1))
                                .map(|s| s.trim());

                            if let Some(range) = range_header {
                                // Parse range: e.g. "bytes=0-100" or "bytes=0-0"
                                if let Some(spec) = range.strip_prefix("bytes=") {
                                    let parts: Vec<&str> = spec.split('-').collect();
                                    let start: usize = parts[0].parse().unwrap_or(0);
                                    let end: usize = if parts.len() > 1 && !parts[1].is_empty() {
                                        parts[1].parse().unwrap_or(total_len - 1)
                                    } else {
                                        total_len - 1
                                    };

                                    let slice_len = if end >= start && start < total_len {
                                        (end - start + 1).min(total_len - start)
                                    } else {
                                        0
                                    };

                                    let resp_header = format!(
                                        "HTTP/1.1 206 Partial Content\r\n\
                                         Content-Type: application/octet-stream\r\n\
                                         Accept-Ranges: bytes\r\n\
                                         Content-Range: bytes {}-{}/{}\r\n\
                                         Content-Length: {}\r\n\
                                         Connection: close\r\n\r\n",
                                        start, start + slice_len - 1, total_len, slice_len
                                    );

                                    let _ = socket.write_all(resp_header.as_bytes()).await;
                                    if !is_head && slice_len > 0 {
                                        let _ = socket.write_all(&payload_clone[start..start + slice_len]).await;
                                    }
                                }
                            } else {
                                // Normal 200 response
                                let resp_header = format!(
                                    "HTTP/1.1 200 OK\r\n\
                                     Content-Type: application/octet-stream\r\n\
                                     Accept-Ranges: bytes\r\n\
                                     Content-Length: {}\r\n\
                                     Connection: close\r\n\r\n",
                                    total_len
                                );
                                let _ = socket.write_all(resp_header.as_bytes()).await;
                                if !is_head {
                                    let _ = socket.write_all(&payload_clone).await;
                                }
                            }
                        }
                    });
                }
                _ = &mut shutdown_rx => {
                    break;
                }
            }
        }
    });

    (addr, shutdown_tx)
}

#[tokio::test]
async fn test_full_dynamic_download_and_integrity() {
    // Generate 2MB payload
    let mut payload = Vec::with_capacity(2 * 1024 * 1024);
    for i in 0..payload.capacity() {
        payload.push((i % 256) as u8);
    }

    let (addr, shutdown_tx) = start_mock_range_server(payload.clone()).await;
    let url = format!("http://{}/test_payload.bin", addr);

    let temp_dir = std::env::temp_dir().join(format!("opendl_dl_test_{}", uuid::Uuid::new_v4()));
    let output_file = temp_dir.join("downloaded.bin");

    let options = DownloadOptions::new(&url, &output_file)
        .with_connections(4);

    let downloader = Downloader::new(options).unwrap();
    let (handle, join_handle) = downloader.start().await.unwrap();

    let meta = handle.metadata;
    assert_eq!(meta.content_length, Some(payload.len() as u64));
    assert!(meta.accepts_ranges);

    let result = join_handle.await.unwrap();
    assert!(result.is_ok(), "Download task should succeed");

    // Read back and verify byte-for-byte equivalence
    let downloaded_data = std::fs::read(&output_file).expect("Downloaded file should exist");
    assert_eq!(downloaded_data.len(), payload.len());
    assert_eq!(downloaded_data, payload, "File content must match byte-for-byte");

    let _ = shutdown_tx.send(());
    let _ = std::fs::remove_dir_all(temp_dir);
}

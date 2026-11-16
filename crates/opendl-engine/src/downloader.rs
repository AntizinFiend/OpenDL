use anyhow::{Context, Result};
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Client;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::watch;
use tokio::time::sleep;
use tracing::info;

use crate::checkpoint::DownloadCheckpoint;
use crate::probe::probe_url;
use crate::segment_allocator::SegmentAllocator;
use crate::storage::Storage;
use crate::types::{DownloadMetadata, DownloadOptions, DownloadProgress, SegmentProgress};
use crate::worker::{run_worker, WorkerConfig};

#[derive(Clone)]
pub struct DownloadHandle {
    pub metadata: DownloadMetadata,
    pub destination: std::path::PathBuf,
    pub progress_rx: watch::Receiver<DownloadProgress>,
    cancel_flag: Arc<AtomicBool>,
}

impl DownloadHandle {
    pub fn pause(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }
}

pub struct Downloader {
    options: DownloadOptions,
    client: Client,
}

impl Downloader {
    pub fn new(options: DownloadOptions) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(15))
            .pool_max_idle_per_host(32)
            .build()
            .context("Failed to construct reqwest client")?;

        Ok(Self { options, client })
    }

    pub async fn start(self) -> Result<(DownloadHandle, tokio::task::JoinHandle<Result<()>>)> {
        let meta = probe_url(&self.client, &self.options).await?;
        info!(
            filename = %meta.suggested_filename,
            content_length = ?meta.content_length,
            accepts_ranges = meta.accepts_ranges,
            "Target probed successfully"
        );

        let mut dest = self.options.destination.clone();
        if dest.is_dir() || dest.as_os_str().is_empty() {
            dest = dest.join(&meta.suggested_filename);
        } else if dest.file_name().and_then(|f| f.to_str()) == Some("download.bin") && meta.suggested_filename != "download.bin" {
            if let Some(parent) = dest.parent() {
                dest = parent.join(&meta.suggested_filename);
            } else {
                dest = std::path::PathBuf::from(&meta.suggested_filename);
            }
        }

        let storage = Arc::new(Storage::create_or_open(&dest, meta.content_length)?);
        let cancel_flag = Arc::new(AtomicBool::new(false));

        let initial_progress = DownloadProgress {
            total_bytes: meta.content_length,
            downloaded_bytes: 0,
            speed_bytes_per_sec: 0.0,
            active_connections: 0,
            segments: Vec::new(),
            is_complete: false,
            ..Default::default()
        };

        let (progress_tx, progress_rx) = watch::channel(initial_progress);
        let handle = DownloadHandle {
            metadata: meta.clone(),
            destination: dest.clone(),
            progress_rx,
            cancel_flag: cancel_flag.clone(),
        };

        let client = self.client.clone();
        let options = self.options.clone();
        let meta_clone = meta.clone();
        let cancel_clone = cancel_flag.clone();
        let storage_clone = storage.clone();

        let join_handle = tokio::spawn(async move {
            let res = Self::run_download_task(
                client,
                options,
                meta_clone,
                storage_clone,
                cancel_clone,
                progress_tx,
            )
            .await;
            res
        });

        Ok((handle, join_handle))
    }

    async fn run_download_task(
        client: Client,
        options: DownloadOptions,
        meta: DownloadMetadata,
        storage: Arc<Storage>,
        cancel_flag: Arc<AtomicBool>,
        progress_tx: watch::Sender<DownloadProgress>,
    ) -> Result<()> {
        let mut base_headers = HeaderMap::new();
        for (k, v) in &options.headers {
            if let (Ok(hname), Ok(hval)) = (HeaderName::from_str(k), HeaderValue::from_str(v)) {
                base_headers.insert(hname, hval);
            }
        }

        // Check if an existing checkpoint exists for resuming
        let checkpoint = DownloadCheckpoint::load_if_exists(storage.path())?;

        let allocator = if let (Some(total_size), true) = (meta.content_length, meta.accepts_ranges) {
            if let Some(cp) = checkpoint {
                info!("Resuming download from existing checkpoint with {} segments", cp.segments.len());
                Arc::new(SegmentAllocator::from_existing(
                    cp.segments,
                    options.min_split_threshold,
                ))
            } else {
                Arc::new(SegmentAllocator::new_initial(
                    total_size,
                    options.connections,
                    options.min_split_threshold,
                ))
            }
        } else {
            // Single stream fallback
            info!("Single connection stream fallback (ranges unsupported or content length unknown)");
            return Self::run_single_stream(client, options, meta, storage, cancel_flag, progress_tx).await;
        };

        let worker_config = Arc::new(WorkerConfig {
            client: client.clone(),
            url: meta.final_url.clone(),
            base_headers,
            storage: storage.clone(),
            allocator: allocator.clone(),
            cancel_flag: cancel_flag.clone(),
            speed_limit_bytes_per_sec: options.speed_limit_bytes_per_sec,
        });

        // Spawn workers for each initial segment
        let initial_segs = allocator.initial_segments();
        let mut worker_joins = Vec::new();

        for seg in initial_segs {
            let cfg = worker_config.clone();
            worker_joins.push(tokio::spawn(async move {
                run_worker(cfg, seg).await;
            }));
        }

        // Monitoring and progress update loop
        let mut last_instant = Instant::now();
        let mut last_bytes = 0u64;

        loop {
            sleep(Duration::from_millis(300)).await;

            let (downloaded, segments, active_count) = allocator.progress_snapshot();
            let now = Instant::now();
            let elapsed_secs = now.duration_since(last_instant).as_secs_f64();

            let speed = if elapsed_secs > 0.0 {
                (downloaded.saturating_sub(last_bytes) as f64) / elapsed_secs
            } else {
                0.0
            };

            last_instant = now;
            last_bytes = downloaded;

            let is_complete = allocator.is_all_completed();

            let progress = DownloadProgress {
                total_bytes: meta.content_length,
                downloaded_bytes: downloaded,
                speed_bytes_per_sec: speed,
                active_connections: active_count,
                segments: segments.clone(),
                is_complete,
                ..Default::default()
            };

            let _ = progress_tx.send(progress);

            // Periodic checkpoint save if still downloading
            if !is_complete && !cancel_flag.load(Ordering::Relaxed) {
                let now_ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let cp = DownloadCheckpoint {
                    url: options.url.clone(),
                    destination: storage.path().to_path_buf(),
                    total_size: meta.content_length,
                    etag: meta.etag.clone(),
                    last_modified: meta.last_modified.clone(),
                    segments,
                    updated_at_secs: now_ts,
                };
                let _ = cp.save();
            }

            if cancel_flag.load(Ordering::Relaxed) {
                info!("Download was cancelled/paused by user");
                break;
            }

            if is_complete {
                info!("All segments completed. Finalizing file.");
                storage.sync()?;
                DownloadCheckpoint::remove(storage.path());
                break;
            }
        }

        for j in worker_joins {
            let _ = j.await;
        }

        Ok(())
    }

    async fn run_single_stream(
        client: Client,
        options: DownloadOptions,
        meta: DownloadMetadata,
        storage: Arc<Storage>,
        cancel_flag: Arc<AtomicBool>,
        progress_tx: watch::Sender<DownloadProgress>,
    ) -> Result<()> {
        let mut req_headers = HeaderMap::new();
        for (k, v) in &options.headers {
            if let (Ok(hname), Ok(hval)) = (HeaderName::from_str(k), HeaderValue::from_str(v)) {
                req_headers.insert(hname, hval);
            }
        }

        let res = client.get(&meta.final_url).headers(req_headers).send().await?;
        let mut stream = res.bytes_stream();
        let mut downloaded = 0u64;
        let mut last_instant = Instant::now();
        let mut last_bytes = 0u64;

        while let Some(chunk_res) = stream.next().await {
            if cancel_flag.load(Ordering::Relaxed) {
                break;
            }

            let chunk = chunk_res?;
            storage.write_at(downloaded, &chunk)?;
            downloaded += chunk.len() as u64;

            let now = Instant::now();
            if now.duration_since(last_instant) >= Duration::from_millis(300) {
                let elapsed = now.duration_since(last_instant).as_secs_f64();
                let speed = (downloaded - last_bytes) as f64 / elapsed;
                last_instant = now;
                last_bytes = downloaded;

                let _ = progress_tx.send(DownloadProgress {
                    total_bytes: meta.content_length,
                    downloaded_bytes: downloaded,
                    speed_bytes_per_sec: speed,
                    active_connections: 1,
                    segments: vec![SegmentProgress {
                        id: 0,
                        start: 0,
                        current: downloaded,
                        end: meta.content_length.unwrap_or(downloaded),
                        is_active: true,
                        is_completed: false,
                    }],
                    is_complete: false,
                    ..Default::default()
                });
            }
        }

        storage.sync()?;
        let _ = progress_tx.send(DownloadProgress {
            total_bytes: meta.content_length,
            downloaded_bytes: downloaded,
            speed_bytes_per_sec: 0.0,
            active_connections: 0,
            segments: vec![SegmentProgress {
                id: 0,
                start: 0,
                current: downloaded,
                end: downloaded,
                is_active: false,
                is_completed: true,
            }],
            is_complete: true,
            ..Default::default()
        });

        Ok(())
    }
}

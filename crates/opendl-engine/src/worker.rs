use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, RANGE};
use reqwest::{Client, StatusCode};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::segment_allocator::{Segment, SegmentAllocator};
use crate::storage::Storage;

pub struct WorkerConfig {
    pub client: Client,
    pub url: String,
    pub base_headers: HeaderMap,
    pub storage: Arc<Storage>,
    pub allocator: Arc<SegmentAllocator>,
    pub cancel_flag: Arc<AtomicBool>,
    pub speed_limit_bytes_per_sec: Option<u64>,
}

pub async fn run_worker(config: Arc<WorkerConfig>, mut segment: Arc<Segment>) {
    let client = &config.client;
    let url = &config.url;
    let storage = &config.storage;
    let allocator = &config.allocator;
    let cancel = &config.cancel_flag;

    loop {
        if cancel.load(Ordering::Relaxed) {
            segment.is_active.store(false, Ordering::Relaxed);
            break;
        }

        segment.is_active.store(true, Ordering::Relaxed);
        let start_offset = segment.current.load(Ordering::SeqCst);
        let end_offset = segment.end.load(Ordering::SeqCst);

        if start_offset <= end_offset {
            debug!(
                segment_id = segment.id,
                range = format!("{}-{}", start_offset, end_offset),
                "Worker starting range request"
            );

            let mut req_headers = config.base_headers.clone();
            let range_val = format!("bytes={}-{}", start_offset, end_offset);
            if let Ok(hval) = HeaderValue::from_str(&range_val) {
                req_headers.insert(RANGE, hval);
            }

            match client.get(url).headers(req_headers).send().await {
                Ok(res) => {
                    let status = res.status();
                    if status == StatusCode::PARTIAL_CONTENT || status == StatusCode::OK {
                        let mut stream = res.bytes_stream();
                        let mut current_offset = start_offset;

                        while let Some(chunk_res) = stream.next().await {
                            if cancel.load(Ordering::Relaxed) {
                                break;
                            }

                            match chunk_res {
                                Ok(chunk) => {
                                    let chunk_len = chunk.len() as u64;
                                    let dynamic_end = segment.end.load(Ordering::SeqCst);

                                    // If dynamic split happened while streaming, don't write past new end
                                    let bytes_to_write = if current_offset + chunk_len > dynamic_end + 1 {
                                        (dynamic_end + 1).saturating_sub(current_offset) as usize
                                    } else {
                                        chunk.len()
                                    };

                                    if bytes_to_write > 0 {
                                        if let Err(err) = storage.write_at(current_offset, &chunk[..bytes_to_write]) {
                                            error!(error = %err, segment_id = segment.id, "Disk write error");
                                            break;
                                        }

                                        current_offset += bytes_to_write as u64;
                                        segment.current.store(current_offset, Ordering::Release);
                                    }

                                    // If we reached or exceeded the dynamic end, stop streaming this segment
                                    if current_offset > dynamic_end {
                                        break;
                                    }
                                }
                                Err(err) => {
                                    warn!(error = %err, segment_id = segment.id, "Stream read error; pausing chunk");
                                    sleep(Duration::from_millis(500)).await;
                                    break;
                                }
                            }
                        }

                        let final_end = segment.end.load(Ordering::SeqCst);
                        if current_offset > final_end {
                            segment.is_completed.store(true, Ordering::SeqCst);
                            info!(segment_id = segment.id, "Segment completed");
                        }
                    } else {
                        warn!(
                            status = %status,
                            segment_id = segment.id,
                            "Server responded with non-partial status"
                        );
                        sleep(Duration::from_millis(500)).await;
                    }
                }
                Err(err) => {
                    warn!(error = %err, segment_id = segment.id, "Request failed");
                    sleep(Duration::from_millis(500)).await;
                }
            }
        } else {
            segment.is_completed.store(true, Ordering::SeqCst);
        }

        segment.is_active.store(false, Ordering::Relaxed);

        if cancel.load(Ordering::Relaxed) {
            break;
        }

        // Dynamic Split Rebalancing:
        // Try to obtain a new sub-range from the largest ongoing active segment
        if let Some(new_segment) = allocator.try_split_largest() {
            debug!(
                old_id = segment.id,
                new_id = new_segment.id,
                "Worker reallocated to dynamically split segment"
            );
            segment = new_segment;
        } else {
            // No segments remain large enough to split; worker gracefully retires
            debug!(segment_id = segment.id, "No further work to split; worker exiting");
            break;
        }
    }
}

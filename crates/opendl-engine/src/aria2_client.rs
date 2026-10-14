use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

use crate::types::{DownloadProgress, SegmentProgress};

#[derive(Debug, Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'static str,
    id: u64,
    method: &'a str,
    params: Value,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    #[allow(dead_code)]
    id: Option<Value>,
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Aria2 RPC Error ({}): {}", self.code, self.message)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Aria2Uri {
    pub uri: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Aria2File {
    pub index: String,
    pub path: String,
    pub length: String,
    pub completed_length: String,
    pub selected: String,
    pub uris: Option<Vec<Aria2Uri>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Aria2Status {
    pub gid: String,
    pub status: String, // "active", "waiting", "paused", "error", "complete", "removed"
    pub total_length: String,
    pub completed_length: String,
    pub download_speed: String,
    pub upload_speed: Option<String>,
    pub piece_length: Option<String>,
    pub num_pieces: Option<String>,
    pub connections: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub dir: Option<String>,
    pub files: Option<Vec<Aria2File>>,
    pub bitfield: Option<String>,
}

impl Aria2Status {
    /// Convert aria2 telemetry into OpenDL's DownloadProgress structure
    pub fn to_download_progress(&self) -> DownloadProgress {
        let total: u64 = self.total_length.parse().unwrap_or(0);
        let completed: u64 = self.completed_length.parse().unwrap_or(0);
        let speed: f64 = self.download_speed.parse().unwrap_or(0.0);
        let conns: usize = self.connections.as_deref().unwrap_or("0").parse().unwrap_or(0);
        let is_complete = self.status == "complete";
        let is_downloading = self.status == "active";

        let total_bytes = if total > 0 { Some(total) } else { None };

        let segments = self.synthesize_segments(total, completed, speed, is_downloading, is_complete);

        DownloadProgress {
            total_bytes,
            downloaded_bytes: completed,
            speed_bytes_per_sec: speed,
            active_connections: conns,
            segments,
            is_complete,
        }
    }

    /// Convert aria2 bitfield into 8 or 16 segment lanes for UI visualization
    fn synthesize_segments(
        &self,
        total: u64,
        completed: u64,
        _total_speed: f64,
        is_downloading: bool,
        is_complete: bool,
    ) -> Vec<SegmentProgress> {
        if total == 0 {
            return vec![SegmentProgress {
                id: 0,
                start: 0,
                current: completed,
                end: completed.max(1),
                is_active: is_downloading,
                is_completed: is_complete,
            }];
        }

        const NUM_LANES: usize = 16;
        let lane_size = (total + NUM_LANES as u64 - 1) / NUM_LANES as u64;

        if let (Some(bf), Some(np_str)) = (&self.bitfield, &self.num_pieces) {
            if let Ok(num_pieces) = np_str.parse::<usize>() {
                if num_pieces > 0 && !bf.is_empty() {
                    let mut bits = Vec::with_capacity(num_pieces);
                    for ch in bf.chars() {
                        if let Some(digit) = ch.to_digit(16) {
                            for b in (0..4).rev() {
                                if bits.len() < num_pieces {
                                    bits.push(((digit >> b) & 1) == 1);
                                }
                            }
                        }
                    }

                    let pieces_per_lane = (num_pieces + NUM_LANES - 1) / NUM_LANES;
                    let mut segments = Vec::with_capacity(NUM_LANES);

                    for lane_idx in 0..NUM_LANES {
                        let start_p = lane_idx * pieces_per_lane;
                        let end_p = ((lane_idx + 1) * pieces_per_lane).min(num_pieces);

                        if start_p >= num_pieces {
                            break;
                        }

                        let total_p_lane = end_p - start_p;
                        let completed_p_lane = bits[start_p..end_p].iter().filter(|&&b| b).count();

                        let start_byte = lane_idx as u64 * lane_size;
                        let end_byte = ((lane_idx as u64 + 1) * lane_size - 1).min(total.saturating_sub(1));
                        let span = end_byte.saturating_sub(start_byte) + 1;

                        let lane_downloaded = if is_complete {
                            span
                        } else if total_p_lane > 0 {
                            (span * completed_p_lane as u64) / total_p_lane as u64
                        } else {
                            0
                        };

                        let lane_completed = is_complete || lane_downloaded >= span;
                        let lane_active = is_downloading && !lane_completed;

                        segments.push(SegmentProgress {
                            id: lane_idx,
                            start: start_byte,
                            current: start_byte + lane_downloaded,
                            end: end_byte,
                            is_active: lane_active,
                            is_completed: lane_completed,
                        });
                    }

                    return segments;
                }
            }
        }

        // Fallback: divide linearly across 8 lanes
        const FALLBACK_LANES: usize = 8;
        let fb_lane_size = (total + FALLBACK_LANES as u64 - 1) / FALLBACK_LANES as u64;
        let mut segments = Vec::with_capacity(FALLBACK_LANES);

        for i in 0..FALLBACK_LANES {
            let start = i as u64 * fb_lane_size;
            let end = ((i as u64 + 1) * fb_lane_size - 1).min(total.saturating_sub(1));

            let cur = if is_complete {
                end
            } else if completed >= end {
                end
            } else if completed > start {
                completed
            } else {
                start
            };

            let is_done = is_complete || cur >= end;
            let is_act = is_downloading && !is_done && (cur > start || i == 0);

            segments.push(SegmentProgress {
                id: i,
                start,
                current: cur,
                end,
                is_active: is_act,
                is_completed: is_done,
            });
        }

        segments
    }
}

#[derive(Clone)]
pub struct Aria2Client {
    endpoint: String,
    secret_param: String,
    http: Client,
    req_counter: Arc<AtomicU64>,
}

impl Aria2Client {
    pub fn new(port: u16, secret: String) -> Self {
        let endpoint = format!("http://127.0.0.1:{}/jsonrpc", port);
        let secret_param = format!("token:{}", secret);
        let http = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        Self {
            endpoint,
            secret_param,
            http,
            req_counter: Arc::new(AtomicU64::new(1)),
        }
    }

    async fn call_rpc<T: for<'de> Deserialize<'de>>(&self, method: &str, params: Value) -> Result<T> {
        let req_id = self.req_counter.fetch_add(1, Ordering::Relaxed);
        let body = JsonRpcRequest {
            jsonrpc: "2.0",
            id: req_id,
            method,
            params,
        };

        debug!(method = %method, id = req_id, "Executing aria2 JSON-RPC call");

        let res = self
            .http
            .post(&self.endpoint)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Failed to connect to aria2 RPC endpoint at {}", self.endpoint))?;

        let status = res.status();
        let text = res
            .text()
            .await
            .with_context(|| format!("Failed to read response body for method {}", method))?;

        if !status.is_success() {
            if let Ok(err_resp) = serde_json::from_str::<JsonRpcResponse<Value>>(&text) {
                if let Some(e) = err_resp.error {
                    anyhow::bail!("Aria2 RPC Error: {}", e);
                }
            }
            anyhow::bail!("Aria2 RPC HTTP error {}: {}", status, text);
        }

        let rpc_res: JsonRpcResponse<T> = serde_json::from_str(&text)
            .with_context(|| format!("Failed to parse aria2 response: {}", text))?;

        if let Some(err) = rpc_res.error {
            anyhow::bail!("Aria2 RPC Error: {}", err);
        }

        rpc_res
            .result
            .ok_or_else(|| anyhow::anyhow!("Aria2 RPC response has neither result nor error"))
    }

    /// Add a download URI with custom options (headers, destination, split, speed limits)
    pub async fn add_uri(
        &self,
        urls: &[String],
        dir: Option<&Path>,
        filename: Option<&str>,
        headers: &[String],
        max_speed: Option<u64>,
        connections: Option<usize>,
    ) -> Result<String> {
        let mut opts = serde_json::Map::new();

        if let Some(d) = dir {
            opts.insert("dir".to_string(), json!(d.to_string_lossy().to_string()));
        }
        if let Some(f) = filename {
            opts.insert("out".to_string(), json!(f));
        }
        if !headers.is_empty() {
            opts.insert("header".to_string(), json!(headers));
        }
        if let Some(speed) = max_speed {
            if speed > 0 {
                opts.insert("max-download-limit".to_string(), json!(speed.to_string()));
            }
        }
        if let Some(conn) = connections {
            let clamped = conn.clamp(1, 32);
            opts.insert("split".to_string(), json!(clamped.to_string()));
            opts.insert("max-connection-per-server".to_string(), json!(clamped.to_string()));
        }

        let params = json!([self.secret_param, urls, opts]);
        self.call_rpc::<String>("aria2.addUri", params).await
    }

    /// Query full status of a download task by GID
    pub async fn tell_status(&self, gid: &str) -> Result<Aria2Status> {
        let params = json!([self.secret_param, gid]);
        self.call_rpc::<Aria2Status>("aria2.tellStatus", params).await
    }

    /// List all currently active downloads
    pub async fn tell_active(&self) -> Result<Vec<Aria2Status>> {
        let params = json!([self.secret_param]);
        self.call_rpc::<Vec<Aria2Status>>("aria2.tellActive", params).await
    }

    /// List waiting/queued downloads
    pub async fn tell_waiting(&self, offset: i32, num: i32) -> Result<Vec<Aria2Status>> {
        let params = json!([self.secret_param, offset, num]);
        self.call_rpc::<Vec<Aria2Status>>("aria2.tellWaiting", params).await
    }

    /// List stopped/completed/failed downloads
    pub async fn tell_stopped(&self, offset: i32, num: i32) -> Result<Vec<Aria2Status>> {
        let params = json!([self.secret_param, offset, num]);
        self.call_rpc::<Vec<Aria2Status>>("aria2.tellStopped", params).await
    }

    /// Pause a download
    pub async fn pause(&self, gid: &str) -> Result<String> {
        let params = json!([self.secret_param, gid]);
        self.call_rpc::<String>("aria2.pause", params).await
    }

    /// Unpause a download
    pub async fn unpause(&self, gid: &str) -> Result<String> {
        let params = json!([self.secret_param, gid]);
        self.call_rpc::<String>("aria2.unpause", params).await
    }

    /// Remove a download from session
    pub async fn remove(&self, gid: &str) -> Result<String> {
        let params = json!([self.secret_param, gid]);
        self.call_rpc::<String>("aria2.remove", params).await
    }

    /// Dynamically change options for a specific task (e.g. speed limit)
    pub async fn change_option(&self, gid: &str, options: HashMap<String, String>) -> Result<String> {
        let params = json!([self.secret_param, gid, options]);
        self.call_rpc::<String>("aria2.changeOption", params).await
    }

    /// Change global aria2 options (e.g. global speed limits)
    pub async fn change_global_option(&self, options: HashMap<String, String>) -> Result<String> {
        let params = json!([self.secret_param, options]);
        self.call_rpc::<String>("aria2.changeGlobalOption", params).await
    }

    /// Query version
    pub async fn get_version(&self) -> Result<String> {
        let params = json!([self.secret_param]);
        #[derive(Deserialize)]
        struct VersionResult {
            version: String,
        }
        let res: VersionResult = self.call_rpc("aria2.getVersion", params).await?;
        Ok(res.version)
    }

    /// Clean shutdown of aria2
    pub async fn shutdown(&self) -> Result<String> {
        let params = json!([self.secret_param]);
        self.call_rpc::<String>("aria2.shutdown", params).await
    }
}

use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

pub const DEFAULT_MAX_CONNECTIONS: usize = 8;
pub const DEFAULT_MIN_SPLIT_THRESHOLD: u64 = 1024 * 1024; // 1 MB
pub const DEFAULT_BUFFER_SIZE: usize = 64 * 1024; // 64 KB

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadOptions {
    pub url: String,
    pub destination: PathBuf,
    pub connections: usize,
    pub headers: HashMap<String, String>,
    pub min_split_threshold: u64,
    pub speed_limit_bytes_per_sec: Option<u64>,
    #[serde(default)]
    pub select_file: Option<String>,
    #[serde(default)]
    pub seed_ratio: Option<f64>,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            url: String::new(),
            destination: PathBuf::new(),
            connections: DEFAULT_MAX_CONNECTIONS,
            headers: HashMap::new(),
            min_split_threshold: DEFAULT_MIN_SPLIT_THRESHOLD,
            speed_limit_bytes_per_sec: None,
            select_file: None,
            seed_ratio: None,
        }
    }
}

impl DownloadOptions {
    pub fn new(url: impl Into<String>, destination: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            destination: destination.into(),
            ..Default::default()
        }
    }

    pub fn with_connections(mut self, connections: usize) -> Self {
        self.connections = connections.clamp(1, 32);
        self
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn with_speed_limit(mut self, bytes_per_sec: u64) -> Self {
        self.speed_limit_bytes_per_sec = Some(bytes_per_sec);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadMetadata {
    pub url: String,
    pub final_url: String,
    pub content_length: Option<u64>,
    pub accepts_ranges: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub suggested_filename: String,
    #[serde(default)]
    pub referrer: Option<String>,
    #[serde(default)]
    pub cookies: Option<String>,
    #[serde(default)]
    pub is_webpage: bool,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentProgress {
    pub id: usize,
    pub start: u64,
    pub current: u64,
    pub end: u64,
    pub is_active: bool,
    pub is_completed: bool,
}

impl SegmentProgress {
    pub fn downloaded_bytes(&self) -> u64 {
        if self.current >= self.start {
            (self.current - self.start).min(self.total_bytes())
        } else {
            0
        }
    }

    pub fn total_bytes(&self) -> u64 {
        if self.end >= self.start {
            self.end - self.start + 1
        } else {
            0
        }
    }

    pub fn remaining_bytes(&self) -> u64 {
        if self.end >= self.current {
            self.end - self.current + 1
        } else {
            0
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub speed_bytes_per_sec: f64,
    pub active_connections: usize,
    pub segments: Vec<SegmentProgress>,
    pub is_complete: bool,
    #[serde(default)]
    pub upload_speed_bytes_per_sec: f64,
    #[serde(default)]
    pub uploaded_bytes: u64,
    #[serde(default)]
    pub num_seeders: usize,
    #[serde(default)]
    pub num_peers: usize,
    #[serde(default)]
    pub is_seeding: bool,
    #[serde(default)]
    pub info_hash: Option<String>,
    #[serde(default)]
    pub piece_length: Option<u64>,
    #[serde(default)]
    pub num_pieces: Option<u64>,
    #[serde(default)]
    pub torrent_name: Option<String>,
    #[serde(default)]
    pub torrent_comment: Option<String>,
    #[serde(default)]
    pub torrent_creation_date: Option<i64>,
}

impl Default for DownloadProgress {
    fn default() -> Self {
        Self {
            total_bytes: None,
            downloaded_bytes: 0,
            speed_bytes_per_sec: 0.0,
            active_connections: 0,
            segments: Vec::new(),
            is_complete: false,
            upload_speed_bytes_per_sec: 0.0,
            uploaded_bytes: 0,
            num_seeders: 0,
            num_peers: 0,
            is_seeding: false,
            info_hash: None,
            piece_length: None,
            num_pieces: None,
            torrent_name: None,
            torrent_comment: None,
            torrent_creation_date: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadState {
    Probing,
    Downloading,
    Paused,
    Completed,
    Failed,
}

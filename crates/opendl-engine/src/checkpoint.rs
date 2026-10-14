use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

use crate::types::SegmentProgress;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadCheckpoint {
    pub url: String,
    pub destination: PathBuf,
    pub total_size: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub segments: Vec<SegmentProgress>,
    pub updated_at_secs: u64,
}

impl DownloadCheckpoint {
    pub fn state_file_path(destination: &Path) -> PathBuf {
        let mut path = destination.as_os_str().to_os_string();
        path.push(".opendl_state");
        PathBuf::from(path)
    }

    pub fn save(&self) -> Result<()> {
        let state_path = Self::state_file_path(&self.destination);
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&state_path, json)
            .with_context(|| format!("Failed to write checkpoint file: {:?}", state_path))?;
        debug!(checkpoint_path = ?state_path, "Saved download checkpoint");
        Ok(())
    }

    pub fn load_if_exists(destination: &Path) -> Result<Option<Self>> {
        let state_path = Self::state_file_path(destination);
        if !state_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&state_path)
            .with_context(|| format!("Failed to read checkpoint file: {:?}", state_path))?;
        let checkpoint: Self = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse checkpoint JSON from {:?}", state_path))?;

        info!(checkpoint_path = ?state_path, "Loaded existing download checkpoint for resume");
        Ok(Some(checkpoint))
    }

    pub fn remove(destination: &Path) {
        let state_path = Self::state_file_path(destination);
        let _ = std::fs::remove_file(state_path);
    }
}

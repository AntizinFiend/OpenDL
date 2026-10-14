use anyhow::{Context, Result};
use opendl_engine::{DownloadHandle, DownloadOptions, Downloader};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info};
use uuid::Uuid;

use crate::db::{Database, TaskRecord, TaskStatus};
use crate::ipc::IpcMessage;

pub struct DownloadManager {
    db: Arc<Database>,
    default_download_dir: PathBuf,
    active_downloads: Arc<Mutex<HashMap<Uuid, DownloadHandle>>>,
}

impl DownloadManager {
    pub fn new(db: Arc<Database>, download_dir: Option<PathBuf>) -> Self {
        let dir = download_dir.unwrap_or_else(|| {
            dirs_or_default_download_dir()
        });

        Self {
            db,
            default_download_dir: dir,
            active_downloads: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn db(&self) -> &Arc<Database> {
        &self.db
    }

    pub async fn add_from_ipc(&self, msg: IpcMessage, start_now: bool) -> Result<Uuid> {
        let url = msg.url.context("URL is required")?;
        let filename = msg.filename.unwrap_or_else(|| {
            opendl_engine::probe::extract_filename(None, None, &url)
        });

        let category = self.db.classify_category(&filename);
        let dest_dir = self.default_download_dir.join(&category);
        let destination = if let Some(d) = msg.destination {
            PathBuf::from(d)
        } else {
            dest_dir.join(&filename)
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let queue_id = msg.queue_id.unwrap_or_else(|| "main".to_string());
        let task_id = Uuid::new_v4();
        let record = TaskRecord {
            id: task_id,
            url: url.clone(),
            filename: filename.clone(),
            destination: destination.clone(),
            total_size: None,
            downloaded_bytes: 0,
            status: if start_now { TaskStatus::Downloading } else { TaskStatus::Queued },
            category,
            etag: None,
            created_at: now,
            updated_at: now,
            queue_id,
            queue_order: 0,
        };

        self.db.insert_task(&record)?;
        info!(task_id = %task_id, url = %url, filename = %filename, "Added download task to manager");

        if start_now {
            let mut options = DownloadOptions::new(&url, &destination);
            if let Some(c) = msg.connections {
                options = options.with_connections(c);
            }
            if let Some(ref_url) = msg.referrer {
                options = options.with_header("Referer", ref_url);
            }
            if let Some(cookies) = msg.cookies {
                options = options.with_header("Cookie", cookies);
            }
            if let Some(ua) = msg.user_agent {
                options = options.with_header("User-Agent", ua);
            }
            if let Some(hdrs) = msg.headers {
                for (k, v) in hdrs {
                    options = options.with_header(k, v);
                }
            }

            if let Err(e) = self.start_download(task_id, options).await {
                error!(task_id = %task_id, error = %e, "Failed to start download stream immediately");
                let _ = self.db.update_task_progress(task_id, 0, TaskStatus::Failed);
            }
        }

        Ok(task_id)
    }

    pub async fn start_download(&self, task_id: Uuid, options: DownloadOptions) -> Result<()> {
        let downloader = Downloader::new(options)?;
        let (handle, join_handle) = downloader.start().await?;

        // Update task metadata from probed response
        let content_length = handle.metadata.content_length;
        let etag = handle.metadata.etag.clone();
        let suggested_filename = handle.metadata.suggested_filename.clone();
        let real_destination = handle.destination.clone();

        if let Ok(Some(mut record)) = self.db.get_task(task_id) {
            let ext = std::path::Path::new(&suggested_filename)
                .extension()
                .and_then(|e| e.to_str());

            if record.filename == "download.bin" && suggested_filename != "download.bin" {
                record.filename = suggested_filename.clone();
                record.destination = real_destination;
                record.category = self.db.classify_category(&suggested_filename);
            } else if !record.filename.contains('.') && suggested_filename != "download.bin" {
                if let Some(e) = ext {
                    record.filename = format!("{}.{}", record.filename, e);
                    record.destination = record.destination.with_extension(e);
                    record.category = self.db.classify_category(&record.filename);
                } else {
                    record.filename = suggested_filename.clone();
                    record.destination = real_destination;
                    record.category = self.db.classify_category(&suggested_filename);
                }
            }
            record.total_size = content_length;
            record.etag = etag;
            record.status = TaskStatus::Downloading;
            let _ = self.db.insert_task(&record);
        }

        let mut progress_rx = handle.progress_rx.clone();
        {
            let mut active = self.active_downloads.lock();
            active.insert(task_id, handle);
        }

        let db = self.db.clone();
        let active_clone = self.active_downloads.clone();

        tokio::spawn(async move {
            let mut last_saved = 0u64;

            while progress_rx.changed().await.is_ok() {
                let p = progress_rx.borrow().clone();

                // Throttle DB updates to at most once per 256KB or on completion
                if p.downloaded_bytes.saturating_sub(last_saved) > 256 * 1024 || p.is_complete {
                    last_saved = p.downloaded_bytes;
                    let status = if p.is_complete {
                        TaskStatus::Completed
                    } else {
                        TaskStatus::Downloading
                    };
                    let _ = db.update_task_progress(task_id, p.downloaded_bytes, status);
                }

                if p.is_complete {
                    break;
                }
            }

            match join_handle.await {
                Ok(Ok(())) => {
                    info!(task_id = %task_id, "Download task completed successfully");
                    let _ = db.update_task_progress(task_id, last_saved, TaskStatus::Completed);
                }
                Ok(Err(e)) => {
                    error!(task_id = %task_id, error = %e, "Download task failed");
                    let _ = db.update_task_progress(task_id, last_saved, TaskStatus::Failed);
                }
                Err(e) => {
                    error!(task_id = %task_id, error = %e, "Download task panicked");
                    let _ = db.update_task_progress(task_id, last_saved, TaskStatus::Failed);
                }
            }

            active_clone.lock().remove(&task_id);
        });

        Ok(())
    }

    pub fn get_active_progress(&self) -> HashMap<Uuid, opendl_engine::DownloadProgress> {
        let active = self.active_downloads.lock();
        let mut map = HashMap::new();
        for (id, handle) in active.iter() {
            map.insert(*id, handle.progress_rx.borrow().clone());
        }
        map
    }

    pub fn pause_task(&self, task_id: Uuid) -> bool {
        let mut active = self.active_downloads.lock();
        if let Some(handle) = active.remove(&task_id) {
            handle.pause();
            let _ = self.db.update_task_progress(task_id, 0, TaskStatus::Paused);
            true
        } else {
            false
        }
    }

    pub async fn start_task_by_id(&self, task_id: Uuid) -> Result<()> {
        let record = self
            .db
            .get_task(task_id)?
            .ok_or_else(|| anyhow::anyhow!("Task not found: {}", task_id))?;
        let options = DownloadOptions::new(&record.url, &record.destination);
        self.start_download(task_id, options).await
    }

    pub async fn process_queue(&self, queue_id: &str) -> Result<()> {
        let q = match self.db.get_queue(queue_id)? {
            Some(q) => q,
            None => return Ok(()),
        };

        if !q.is_active {
            return Ok(());
        }

        let active_count = self.db.count_downloading_tasks_in_queue(queue_id)?;
        if active_count < q.max_concurrent {
            let slots = q.max_concurrent - active_count;
            let next_tasks = self.db.get_next_queued_tasks_in_queue(queue_id, slots)?;
            for task in next_tasks {
                let options = DownloadOptions::new(&task.url, &task.destination);
                let _ = self.start_download(task.id, options).await;
            }
        }
        Ok(())
    }

    pub async fn process_all_queues(&self) -> Result<()> {
        let queues = self.db.get_all_queues()?;
        for q in queues {
            let _ = self.process_queue(&q.id).await;
        }
        Ok(())
    }

    pub fn pause_queue(&self, queue_id: &str) -> Result<()> {
        self.db.set_queue_active(queue_id, false)?;
        let tasks = self.db.get_all_tasks()?;
        for t in tasks {
            if t.queue_id == queue_id && t.status == TaskStatus::Downloading {
                self.pause_task(t.id);
            }
        }
        Ok(())
    }

    pub async fn start_queue(&self, queue_id: &str) -> Result<()> {
        self.db.set_queue_active(queue_id, true)?;
        self.process_queue(queue_id).await
    }

    pub fn start_scheduler(self: &Arc<Self>) {
        let mgr = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("opendl-scheduler".to_string())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        tracing::error!("Failed to create scheduler runtime: {}", e);
                        return;
                    }
                };

                rt.block_on(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        if let Some(m) = mgr.upgrade() {
                            let _ = m.check_schedules_and_process().await;
                        } else {
                            break;
                        }
                    }
                });
            })
            .ok();
    }

    pub async fn check_schedules_and_process(&self) -> Result<()> {
        let now = chrono_or_system_time_hhmm();
        let queues = self.db.get_all_queues()?;

        for q in queues {
            // Check time-based scheduling
            if let Some(ref start_time) = q.schedule_start {
                if start_time == &now && !q.is_active {
                    let _ = self.db.set_queue_active(&q.id, true);
                }
            }
            if let Some(ref stop_time) = q.schedule_stop {
                if stop_time == &now && q.is_active {
                    let _ = self.pause_queue(&q.id);
                    continue;
                }
            }

            if q.is_active {
                let _ = self.process_queue(&q.id).await;
            }
        }

        Ok(())
    }
}

fn chrono_or_system_time_hhmm() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let sec_in_day = now % 86400;
    let hour = sec_in_day / 3600;
    let min = (sec_in_day % 3600) / 60;
    format!("{:02}:{:02}", hour, min)
}

fn dirs_or_default_download_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            return PathBuf::from(profile).join("Downloads");
        }
    }
    PathBuf::from("Downloads")
}

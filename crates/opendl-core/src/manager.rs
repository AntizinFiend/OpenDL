use anyhow::{Context, Result};
use opendl_engine::{
    Aria2Client, Aria2Process, DownloadOptions, DownloadProgress,
};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::db::{Database, TaskRecord, TaskStatus};
use crate::ipc::IpcMessage;
use crate::power::trigger_power_action;

pub struct DownloadManager {
    db: Arc<Database>,
    default_download_dir: PathBuf,
    _aria2_proc: Arc<Mutex<Option<Aria2Process>>>,
    aria2_client: Aria2Client,
    active_tasks: Arc<Mutex<HashMap<Uuid, String>>>, // task_id -> aria2_gid
    gid_to_task: Arc<Mutex<HashMap<String, Uuid>>>,  // aria2_gid -> task_id
    progress_cache: Arc<Mutex<HashMap<Uuid, DownloadProgress>>>,
    last_db_save: Arc<Mutex<HashMap<Uuid, u64>>>,
}

impl DownloadManager {
    pub fn new(db: Arc<Database>, download_dir: Option<PathBuf>) -> Self {
        let dir = download_dir.unwrap_or_else(dirs_or_default_download_dir);
        let _ = std::fs::create_dir_all(&dir);

        let (proc_opt, client) = match Aria2Process::spawn(None, Some(&dir)) {
            Ok(p) => {
                let client = p.client();
                (Some(p), client)
            }
            Err(e) => {
                error!(error = %e, "Failed to spawn aria2c sidecar. Using fallback RPC port 6800");
                (None, Aria2Client::new(6800, String::new()))
            }
        };

        let mgr = Self {
            db,
            default_download_dir: dir,
            _aria2_proc: Arc::new(Mutex::new(proc_opt)),
            aria2_client: client,
            active_tasks: Arc::new(Mutex::new(HashMap::new())),
            gid_to_task: Arc::new(Mutex::new(HashMap::new())),
            progress_cache: Arc::new(Mutex::new(HashMap::new())),
            last_db_save: Arc::new(Mutex::new(HashMap::new())),
        };

        mgr.start_telemetry_loop();
        mgr
    }

    pub fn db(&self) -> &Arc<Database> {
        &self.db
    }

    pub fn client(&self) -> &Aria2Client {
        &self.aria2_client
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
                error!(task_id = %task_id, error = %e, "Failed to start aria2 download");
                let _ = self.db.update_task_progress(task_id, 0, TaskStatus::Failed);
            }
        }

        Ok(task_id)
    }

    pub async fn start_download(&self, task_id: Uuid, options: DownloadOptions) -> Result<()> {
        let record = self
            .db
            .get_task(task_id)?
            .ok_or_else(|| anyhow::anyhow!("Task not found: {}", task_id))?;

        let dest_parent = record
            .destination
            .parent()
            .unwrap_or_else(|| Path::new(&self.default_download_dir));
        let _ = std::fs::create_dir_all(dest_parent);

        let mut headers = Vec::new();
        for (k, v) in &options.headers {
            headers.push(format!("{}: {}", k, v));
        }

        // Determine queue speed limit if configured
        let mut speed_limit = options.speed_limit_bytes_per_sec;
        if speed_limit.is_none() {
            if let Ok(Some(q)) = self.db.get_queue(&record.queue_id) {
                if q.speed_limit_kbps > 0 {
                    speed_limit = Some(q.speed_limit_kbps * 1024);
                }
            }
        }

        let gid = self
            .aria2_client
            .add_uri(
                &[record.url.clone()],
                Some(dest_parent),
                Some(&record.filename),
                &headers,
                speed_limit,
                Some(options.connections),
            )
            .await
            .with_context(|| format!("Aria2 failed to add URI: {}", record.url))?;

        info!(task_id = %task_id, gid = %gid, "Aria2 download initiated");

        {
            let mut at = self.active_tasks.lock();
            let mut gt = self.gid_to_task.lock();
            at.insert(task_id, gid.clone());
            gt.insert(gid, task_id);
        }

        let _ = self.db.update_task_progress(task_id, record.downloaded_bytes, TaskStatus::Downloading);
        Ok(())
    }

    pub fn pause_task(&self, task_id: Uuid) -> bool {
        let gid = {
            let at = self.active_tasks.lock();
            at.get(&task_id).cloned()
        };

        if let Some(gid) = gid {
            let client = self.aria2_client.clone();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let _ = client.pause(&gid).await;
                });
            } else {
                std::thread::spawn(move || {
                    if let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() {
                        rt.block_on(async move {
                            let _ = client.pause(&gid).await;
                        });
                    }
                });
            }
            let _ = self.db.update_task_progress(task_id, 0, TaskStatus::Paused);
            true
        } else {
            let _ = self.db.update_task_progress(task_id, 0, TaskStatus::Paused);
            true
        }
    }

    pub async fn resume_task(&self, task_id: Uuid) -> Result<bool> {
        let record = self
            .db
            .get_task(task_id)?
            .ok_or_else(|| anyhow::anyhow!("Task not found: {}", task_id))?;

        let gid = {
            let at = self.active_tasks.lock();
            at.get(&task_id).cloned()
        };

        if let Some(gid) = gid {
            match self.aria2_client.unpause(&gid).await {
                Ok(_) => {
                    let _ = self.db.update_task_progress(task_id, record.downloaded_bytes, TaskStatus::Downloading);
                    return Ok(true);
                }
                Err(e) => {
                    debug!(gid = %gid, error = %e, "unpause failed, re-adding download to aria2");
                }
            }
        }

        let options = DownloadOptions::new(&record.url, &record.destination);
        self.start_download(task_id, options).await?;
        Ok(true)
    }

    pub fn delete_task(&self, task_id: Uuid, delete_file: bool) -> Result<()> {
        let gid = {
            let mut at = self.active_tasks.lock();
            let mut gt = self.gid_to_task.lock();
            let mut pc = self.progress_cache.lock();
            pc.remove(&task_id);
            if let Some(g) = at.remove(&task_id) {
                gt.remove(&g);
                Some(g)
            } else {
                None
            }
        };

        if let Some(gid) = gid {
            let client = self.aria2_client.clone();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let _ = client.remove(&gid).await;
                    let _ = client.remove_download_result(&gid).await;
                });
            } else {
                std::thread::spawn(move || {
                    if let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() {
                        rt.block_on(async move {
                            let _ = client.remove(&gid).await;
                            let _ = client.remove_download_result(&gid).await;
                        });
                    }
                });
            }
        }

        if let Ok(Some(record)) = self.db.get_task(task_id) {
            if delete_file && record.destination.exists() {
                let _ = std::fs::remove_file(&record.destination);
            }
        }

        self.db.delete_task(task_id)?;
        Ok(())
    }

    pub fn get_active_progress(&self) -> HashMap<Uuid, DownloadProgress> {
        let pc = self.progress_cache.lock();
        pc.clone()
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

    fn start_telemetry_loop(&self) {
        let client = self.aria2_client.clone();
        let db = self.db.clone();
        let active_tasks = self.active_tasks.clone();
        let gid_to_task = self.gid_to_task.clone();
        let progress_cache = self.progress_cache.clone();
        let last_db_save = self.last_db_save.clone();

        std::thread::Builder::new()
            .name("opendl-telemetry".to_string())
            .spawn(move || {
                if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    rt.block_on(async move {
                        let mut interval = tokio::time::interval(Duration::from_millis(200));

                        loop {
                            interval.tick().await;

                            // 1. Poll active downloads
                            if let Ok(active) = client.tell_active().await {
                                for status in active {
                                    let task_id = {
                                        let gt = gid_to_task.lock();
                                        gt.get(&status.gid).cloned()
                                    };

                                    if let Some(id) = task_id {
                                        let progress = status.to_download_progress();
                                        let dl = progress.downloaded_bytes;

                                        // Update in-memory telemetry cache for high-frequency UI rendering
                                        {
                                            let mut pc = progress_cache.lock();
                                            pc.insert(id, progress);
                                        }

                                        // Throttled DB persistence (at least 2MB change to prevent SSD thrashing)
                                        let should_save = {
                                            let mut saves = last_db_save.lock();
                                            let last = saves.get(&id).cloned().unwrap_or(0);
                                            if dl.saturating_sub(last) > 2 * 1024 * 1024 {
                                                saves.insert(id, dl);
                                                true
                                            } else {
                                                false
                                            }
                                        };

                                        if should_save {
                                            let _ = db.update_task_progress(id, dl, TaskStatus::Downloading);
                                        }
                                    }
                                }
                            }

                            // 2. Poll stopped / completed / error downloads
                            if let Ok(stopped) = client.tell_stopped(0, 50).await {
                                for status in stopped {
                                    let task_id = {
                                        let gt = gid_to_task.lock();
                                        gt.get(&status.gid).cloned()
                                    };

                                    if let Some(id) = task_id {
                                        let is_complete = status.status == "complete";
                                        let dl: u64 = status.completed_length.parse().unwrap_or(0);

                                        if is_complete {
                                            info!(task_id = %id, gid = %status.gid, "Task marked complete in aria2");
                                            let _ = db.update_task_progress(id, dl, TaskStatus::Completed);
                                            {
                                                let mut pc = progress_cache.lock();
                                                if let Some(p) = pc.get_mut(&id) {
                                                    p.is_complete = true;
                                                    p.downloaded_bytes = dl;
                                                    p.speed_bytes_per_sec = 0.0;
                                                    p.active_connections = 0;
                                                }
                                            }

                                            // Check if queue finished and execute power action if configured
                                            if let Ok(Some(task)) = db.get_task(id) {
                                                if let Ok(active_in_queue) = db.count_downloading_tasks_in_queue(&task.queue_id) {
                                                    if active_in_queue == 0 {
                                                        if let Ok(Some(q)) = db.get_queue(&task.queue_id) {
                                                            if let Some(ref action) = q.on_complete_action {
                                                                if action != "none" {
                                                                    info!(queue = %q.name, action = %action, "Queue completed! Executing power action");
                                                                    let _ = trigger_power_action(action);
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        } else if status.status == "error" {
                                            warn!(task_id = %id, gid = %status.gid, err = ?status.error_message, "Task marked failed in aria2");
                                            let _ = db.update_task_progress(id, dl, TaskStatus::Failed);
                                        }

                                        // Clean up active mappings and remove stopped result from aria2
                                        {
                                            let mut at = active_tasks.lock();
                                            let mut gt = gid_to_task.lock();
                                            at.remove(&id);
                                            gt.remove(&status.gid);
                                        }
                                        let _ = client.remove_download_result(&status.gid).await;
                                    }
                                }
                            }
                        }
                    });
                }
            })
            .ok();
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
                        error!("Failed to create scheduler runtime: {}", e);
                        return;
                    }
                };

                rt.block_on(async move {
                    loop {
                        tokio::time::sleep(Duration::from_secs(1)).await;
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

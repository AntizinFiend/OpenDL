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

fn is_generic_filename(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    if lower.is_empty()
        || lower == "download.bin"
        || lower == "download"
        || lower == "download.php"
        || lower == "file"
        || lower == "unnamed"
        || lower == "index.html"
        || lower == "index.php"
    {
        return true;
    }
    let script_exts = [".php", ".jsp", ".asp", ".aspx", ".cgi", ".do", ".action"];
    if script_exts.iter().any(|ext| lower.ends_with(ext)) {
        return true;
    }
    false
}

fn extract_magnet_name(magnet_url: &str) -> Option<String> {
    for part in magnet_url.split('&') {
        let p = if part.starts_with("magnet:?") { &part[8..] } else { part };
        if p.starts_with("dn=") {
            let encoded = &p[3..];
            let mut s = String::new();
            let bytes = encoded.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'+' {
                    s.push(' ');
                    i += 1;
                } else if bytes[i] == b'%' && i + 2 < bytes.len() {
                    if let Ok(val) = u8::from_str_radix(std::str::from_utf8(&bytes[i+1..i+3]).unwrap_or(""), 16) {
                        s.push(val as char);
                        i += 3;
                    } else {
                        s.push(bytes[i] as char);
                        i += 1;
                    }
                } else {
                    s.push(bytes[i] as char);
                    i += 1;
                }
            }
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
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
            db: db.clone(),
            default_download_dir: dir,
            _aria2_proc: Arc::new(Mutex::new(proc_opt)),
            aria2_client: client,
            active_tasks: Arc::new(Mutex::new(HashMap::new())),
            gid_to_task: Arc::new(Mutex::new(HashMap::new())),
            progress_cache: Arc::new(Mutex::new(HashMap::new())),
            last_db_save: Arc::new(Mutex::new(HashMap::new())),
        };

        // Reconcile interrupted downloads: if any task is marked Downloading in DB, set to Paused so it can be cleanly resumed
        if let Ok(tasks) = db.get_all_tasks() {
            for t in tasks {
                if t.status == TaskStatus::Downloading {
                    let _ = db.update_task_progress(t.id, t.downloaded_bytes, TaskStatus::Paused);
                }
            }
        }

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
        let raw_filename = msg.filename.clone().unwrap_or_else(|| {
            opendl_engine::probe::extract_filename(None, None, &url)
        });

        let is_magnet = url.starts_with("magnet:");
        let is_torrent_file = url.ends_with(".torrent") || Path::new(&url).extension().map(|e| e == "torrent").unwrap_or(false);

        let filename = if is_magnet {
            if let Some(dn) = extract_magnet_name(&url) {
                dn
            } else if let Some(ref f) = msg.filename {
                if !is_generic_filename(f) {
                    f.clone()
                } else {
                    "Magnet Download".to_string()
                }
            } else {
                "Magnet Download".to_string()
            }
        } else if is_torrent_file && Path::new(&url).exists() {
            Path::new(&url)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "Torrent Download".to_string())
        } else {
            let mut fname = Path::new(&raw_filename)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or(raw_filename);
            if fname.is_empty() || fname == "." {
                fname = opendl_engine::probe::extract_filename(None, None, &url);
            }
            fname
        };

        let category = if is_magnet || is_torrent_file {
            "Torrents".to_string()
        } else {
            self.db.classify_category(&filename)
        };
        let dest_dir = self.default_download_dir.join(&category);
        let destination = if let Some(d) = msg.destination {
            let p = PathBuf::from(&d);
            let s = d.replace('/', "\\");
            if p.is_dir() || s.ends_with('\\') {
                p.join(&filename)
            } else if let Some(parent) = p.parent() {
                let fname = p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| filename.clone());
                parent.join(fname)
            } else {
                dest_dir.join(&filename)
            }
        } else {
            dest_dir.join(&filename)
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut queue_id = msg.queue_id.unwrap_or_else(|| "main".to_string());
        if let Ok(queues) = self.db.get_all_queues() {
            if let Some(q) = queues.iter().find(|q| q.id == queue_id || q.name.eq_ignore_ascii_case(&queue_id)) {
                queue_id = q.id.clone();
            }
        }

        let mut target_url = url.clone();
        let mut target_referrer = msg.referrer.clone();
        let mut target_cookies = msg.cookies.clone();
        let mut target_ua = msg.user_agent.clone();
        let mut target_headers = msg.headers.clone().unwrap_or_default();

        if opendl_engine::resolver::is_mediafire_url(&url)
            || opendl_engine::resolver::is_google_drive_url(&url)
            || opendl_engine::resolver::is_dropbox_url(&url)
        {
            if let Ok(res) = opendl_engine::resolver::resolve_download_url(&url).await {
                info!(original = %url, direct = %res.direct_url, "Manager resolved direct download URL via engine resolver");
                target_url = res.direct_url;
                if target_referrer.is_none() {
                    target_referrer = res.referrer;
                }
                if target_cookies.is_none() {
                    target_cookies = res.cookies;
                }
                if target_ua.is_none() {
                    target_ua = res.user_agent;
                }
                for (k, v) in res.headers {
                    target_headers.entry(k).or_insert(v);
                }
            }
        }

        let custom_headers_json = if target_headers.is_empty() {
            None
        } else {
            serde_json::to_string(&target_headers).ok()
        };

        let task_id = Uuid::new_v4();
        let record = TaskRecord {
            id: task_id,
            url: target_url.clone(),
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
            referrer: target_referrer.clone(),
            cookies: target_cookies.clone(),
            user_agent: target_ua.clone(),
            custom_headers: custom_headers_json,
        };

        self.db.insert_task(&record)?;
        info!(task_id = %task_id, url = %target_url, filename = %filename, "Added download task to manager");

        if start_now {
            let mut options = self.build_download_options(&record);
            if let Some(c) = msg.connections {
                options = options.with_connections(c);
            }

            if let Err(e) = self.start_download(task_id, options).await {
                error!(task_id = %task_id, error = %e, "Failed to start aria2 download");
                let _ = self.db.update_task_progress(task_id, 0, TaskStatus::Failed);
            }
        }

        Ok(task_id)
    }


    pub fn build_download_options(&self, task: &TaskRecord) -> DownloadOptions {
        let mut options = DownloadOptions::new(&task.url, &task.destination);
        if let Some(ref r) = task.referrer {
            options = options.with_header("Referer", r);
        }
        if let Some(ref c) = task.cookies {
            options = options.with_header("Cookie", c);
        }
        if let Some(ref ua) = task.user_agent {
            options = options.with_header("User-Agent", ua);
        }
        if let Some(ref h_str) = task.custom_headers {
            if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(h_str) {
                for (k, v) in map {
                    if k.eq_ignore_ascii_case("select-file") || k.eq_ignore_ascii_case("x-select-files") {
                        options.select_file = Some(v);
                    } else if k.eq_ignore_ascii_case("seed-ratio") || k.eq_ignore_ascii_case("x-seed-ratio") {
                        if let Ok(r) = v.parse::<f64>() {
                            options.seed_ratio = Some(r);
                        }
                    } else {
                        options = options.with_header(k, v);
                    }
                }
            }
        }
        options
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

        // Always inject browser User-Agent and standard web headers for maximum compatibility
        let has_ua = options.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("user-agent"));
        let has_accept = options.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("accept"));
        let has_lang = options.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("accept-language"));
        let has_ref = options.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("referer"));

        if !has_ua {
            headers.push(format!("User-Agent: {}", opendl_engine::resolver::DEFAULT_BROWSER_UA));
        }
        if !has_accept {
            headers.push("Accept: text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8".to_string());
        }
        if !has_lang {
            headers.push("Accept-Language: en-US,en;q=0.9".to_string());
        }
        if !has_ref {
            if let Ok(parsed) = url::Url::parse(&record.url) {
                if let Some(host) = parsed.host_str() {
                    headers.push(format!("Referer: {}://{}/", parsed.scheme(), host));
                }
            }
        }
        headers.push("Sec-Fetch-Dest: document".to_string());
        headers.push("Sec-Fetch-Mode: navigate".to_string());
        headers.push("Sec-Fetch-Site: cross-site".to_string());
        headers.push("Upgrade-Insecure-Requests: 1".to_string());


        // Determine queue speed limit if configured
        let mut speed_limit = options.speed_limit_bytes_per_sec;
        if speed_limit.is_none() {
            if let Ok(Some(q)) = self.db.get_queue(&record.queue_id) {
                if q.speed_limit_kbps > 0 {
                    speed_limit = Some(q.speed_limit_kbps * 1024);
                }
            }
        }

        let clean_filename = Path::new(&record.filename)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| record.filename.clone());

        let out_filename = if is_generic_filename(&clean_filename) {
            None
        } else {
            Some(clean_filename.as_str())
        };

        let mut custom_torrent_opts = HashMap::new();
        if let Some(ref sel) = options.select_file {
            if !sel.trim().is_empty() {
                custom_torrent_opts.insert("select-file".to_string(), sel.clone());
            }
        }
        if let Some(ratio) = options.seed_ratio {
            custom_torrent_opts.insert("seed-ratio".to_string(), ratio.to_string());
        }
        let custom_opts_opt = if custom_torrent_opts.is_empty() { None } else { Some(custom_torrent_opts) };

        let gid = if record.url.starts_with("magnet:") {
            self.aria2_client
                .add_uri(
                    &[record.url.clone()],
                    Some(dest_parent),
                    None,
                    &headers,
                    speed_limit,
                    Some(options.connections),
                )
                .await
                .with_context(|| format!("Aria2 failed to add magnet URI: {}", record.url))?
        } else if (record.url.ends_with(".torrent") || record.url.starts_with("file:")) && Path::new(&record.url).exists() {
            let torrent_bytes = std::fs::read(&record.url)
                .with_context(|| format!("Failed to read .torrent file at {}", record.url))?;
            self.aria2_client
                .add_torrent(&torrent_bytes, Some(dest_parent), custom_opts_opt)
                .await
                .with_context(|| format!("Aria2 failed to add torrent: {}", record.url))?
        } else {
            self.aria2_client
                .add_uri(
                    &[record.url.clone()],
                    Some(dest_parent),
                    out_filename,
                    &headers,
                    speed_limit,
                    Some(options.connections),
                )
                .await
                .with_context(|| format!("Aria2 failed to add URI: {}", record.url))?
        };

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
        let current_dl = self
            .progress_cache
            .lock()
            .get(&task_id)
            .map(|p| p.downloaded_bytes)
            .or_else(|| self.db.get_task(task_id).ok().flatten().map(|t| t.downloaded_bytes))
            .unwrap_or(0);

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
        }

        {
            let mut pc = self.progress_cache.lock();
            if let Some(p) = pc.get_mut(&task_id) {
                p.speed_bytes_per_sec = 0.0;
                p.active_connections = 0;
            }
        }

        let _ = self.db.update_task_progress(task_id, current_dl, TaskStatus::Paused);
        true
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

        let options = self.build_download_options(&record);
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
            if delete_file {
                if record.destination.exists() {
                    let _ = std::fs::remove_file(&record.destination);
                }
                let aria2_meta = PathBuf::from(format!("{}.aria2", record.destination.to_string_lossy()));
                if aria2_meta.exists() {
                    let _ = std::fs::remove_file(&aria2_meta);
                }
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

        let tasks = self.db.get_actionable_tasks_in_queue(queue_id)?;
        let max = q.max_concurrent.max(1);

        for (idx, task) in tasks.iter().enumerate() {
            if idx < max {
                // Should be downloading
                if task.status != TaskStatus::Downloading {
                    info!(task_id = %task.id, filename = %task.filename, "Starting task promoted in queue");
                    let options = self.build_download_options(task);
                    let _ = self.start_download(task.id, options).await;
                }
            } else {
                // Beyond max_concurrent limit: pause if currently downloading
                if task.status == TaskStatus::Downloading {
                    info!(task_id = %task.id, filename = %task.filename, "Pausing task displaced in queue");
                    self.pause_task(task.id);
                }
            }
        }
        Ok(())
    }

    pub async fn reorder_queue_and_reschedule(&self, queue_id: &str, task_ids_in_order: &[Uuid]) -> Result<()> {
        self.db.reorder_queue_tasks(queue_id, task_ids_in_order)?;
        self.process_queue(queue_id).await
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
                            match client.tell_active().await {
                                Ok(active) => {
                                    for status in active {
                                        let task_id = {
                                            let mut gt = gid_to_task.lock();
                                            if let Some(&id) = gt.get(&status.gid) {
                                                Some(id)
                                            } else if let Some(ref parent_gid) = status.following {
                                                if let Some(&id) = gt.get(parent_gid) {
                                                    gt.insert(status.gid.clone(), id);
                                                    Some(id)
                                                } else {
                                                    None
                                                }
                                            } else {
                                                let at = active_tasks.lock();
                                                if at.len() == 1 {
                                                    let id = *at.keys().next().unwrap();
                                                    gt.insert(status.gid.clone(), id);
                                                    Some(id)
                                                } else {
                                                    None
                                                }
                                            }
                                        };

                                        if let Some(id) = task_id {
                                            let total_len = status.total_length.as_deref().and_then(|s| s.parse::<u64>().ok()).filter(|&l| l > 0);
                                            let mut actual_name_opt = None;
                                            let mut actual_path_opt = None;

                                            // Update resolved filename from aria2 if it learned the real name via Content-Disposition/redirects
                                            if let Some(ref files) = status.files {
                                                if let Some(first) = files.first() {
                                                    if let Some(ref p) = first.path {
                                                        if !p.is_empty() {
                                                            let actual_path = PathBuf::from(p);
                                                            if let Some(actual_name) = actual_path.file_name().map(|f| f.to_string_lossy().to_string()) {
                                                                if !actual_name.is_empty() {
                                                                    actual_name_opt = Some(actual_name);
                                                                    actual_path_opt = Some(actual_path);
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            if let Ok(Some(task)) = db.get_task(id) {
                                                let has_new_name = actual_name_opt.is_some() && actual_name_opt.as_ref() != Some(&task.filename);
                                                let has_new_size = total_len.is_some() && task.total_size.is_none();
                                                if has_new_name || has_new_size {
                                                    let updated_name = actual_name_opt.unwrap_or(task.filename);
                                                    let updated_path = actual_path_opt.unwrap_or(task.destination);
                                                    let cat = db.classify_category(&updated_name);
                                                    let _ = db.update_task_metadata(id, &updated_name, &updated_path, total_len.or(task.total_size), &cat, None);
                                                }
                                            }

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
                                Err(e) => {
                                    debug!(error = %e, "tell_active error in telemetry loop");
                                }
                            }

                            // 2. Poll stopped / completed / error downloads
                            match client.tell_stopped(0, 50).await {
                                Ok(stopped) => {
                                    for status in stopped {
                                        let task_id = {
                                            let gt = gid_to_task.lock();
                                            gt.get(&status.gid).cloned()
                                        };

                                        if let Some(id) = task_id {
                                            if let Some(ref files) = status.files {
                                                if let Some(first) = files.first() {
                                                    if let Some(ref p) = first.path {
                                                        if !p.is_empty() {
                                                            let actual_path = PathBuf::from(p);
                                                            if let Some(actual_name) = actual_path.file_name().map(|f| f.to_string_lossy().to_string()) {
                                                                if !actual_name.is_empty() {
                                                                    if let Ok(Some(task)) = db.get_task(id) {
                                                                        if task.filename != actual_name {
                                                                            let cat = db.classify_category(&actual_name);
                                                                            let total_len = status.total_length.as_deref().and_then(|s| s.parse::<u64>().ok()).filter(|&l| l > 0);
                                                                            let _ = db.update_task_metadata(id, &actual_name, &actual_path, total_len.or(task.total_size), &cat, None);
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            // If this download spawned followed downloads (e.g. magnet metadata finished and torrent payload started),
                                            // transition GID mappings to the child download instead of marking complete!
                                            if let Some(ref children) = status.followed_by {
                                                if !children.is_empty() {
                                                    debug!(task_id = %id, gid = %status.gid, children = ?children, "Magnet metadata completed, transitioning to child GID");
                                                    let mut gt = gid_to_task.lock();
                                                    let mut at = active_tasks.lock();
                                                    for child in children {
                                                        gt.insert(child.clone(), id);
                                                        at.insert(id, child.clone());
                                                    }
                                                    gt.remove(&status.gid);
                                                    let _ = client.remove_download_result(&status.gid).await;
                                                    continue;
                                                }
                                            }

                                            let is_complete = status.status == "complete";
                                            let dl: u64 = status.completed_length.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0);

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
                                Err(e) => {
                                    debug!(error = %e, "tell_stopped error in telemetry loop");
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

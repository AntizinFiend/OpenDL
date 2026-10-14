#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use opendl_core::db::{Database, QueueRecord, TaskRecord, TaskStatus};
use opendl_core::ipc::{IpcMessage, IpcResponse, IpcServer, DEFAULT_PIPE_NAME};
use opendl_core::manager::DownloadManager;
use opendl_engine::DownloadProgress;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

type AppManager = Arc<DownloadManager>;

#[tauri::command]
async fn add_task(
    manager: State<'_, AppManager>,
    url: String,
    filename: Option<String>,
    start_now: Option<bool>,
    connections: Option<usize>,
    destination: Option<String>,
    queue_id: Option<String>,
) -> Result<TaskRecord, String> {
    let should_start = start_now.unwrap_or(true);
    let msg = IpcMessage {
        action: "add_download".to_string(),
        url: Some(url),
        filename,
        destination,
        referrer: None,
        cookies: None,
        user_agent: None,
        headers: None,
        task_id: None,
        connections,
        queue_id,
    };

    let id = manager
        .add_from_ipc(msg, should_start)
        .await
        .map_err(|e| e.to_string())?;

    let record = manager
        .db()
        .get_task(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Failed to retrieve inserted task".to_string())?;

    Ok(record)
}

#[tauri::command]
async fn list_tasks(manager: State<'_, AppManager>) -> Result<Vec<TaskRecord>, String> {
    manager.db().get_all_tasks().map_err(|e| e.to_string())
}

#[tauri::command]
async fn pause_task(manager: State<'_, AppManager>, id: String) -> Result<bool, String> {
    let task_id = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    Ok(manager.pause_task(task_id))
}

#[tauri::command]
async fn resume_task(manager: State<'_, AppManager>, id: String) -> Result<bool, String> {
    let task_id = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let record = manager
        .db()
        .get_task(task_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Task not found".to_string())?;

    let options = opendl_engine::DownloadOptions::new(&record.url, &record.destination);
    manager
        .start_download(task_id, options)
        .await
        .map_err(|e| e.to_string())?;

    Ok(true)
}

#[tauri::command]
async fn delete_task(
    manager: State<'_, AppManager>,
    id: String,
    delete_file: bool,
) -> Result<bool, String> {
    let task_id = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    if let Ok(Some(record)) = manager.db().get_task(task_id) {
        if delete_file && record.destination.exists() {
            let _ = std::fs::remove_file(&record.destination);
        }
    }
    manager.pause_task(task_id);
    manager.db().delete_task(task_id).map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
async fn get_all_progress(
    manager: State<'_, AppManager>,
) -> Result<HashMap<String, DownloadProgress>, String> {
    let mut map = HashMap::new();

    // 1. Populate baseline progress for all tasks from SQLite
    let tasks = manager.db().get_all_tasks().map_err(|e| e.to_string())?;
    for t in tasks {
        map.insert(
            t.id.to_string(),
            DownloadProgress {
                total_bytes: t.total_size,
                downloaded_bytes: t.downloaded_bytes,
                speed_bytes_per_sec: 0.0,
                active_connections: 0,
                segments: Vec::new(),
                is_complete: t.status == TaskStatus::Completed,
            },
        );
    }

    // 2. Overlay live streaming progress from active in-memory downloads
    let active = manager.get_active_progress();
    for (id, prog) in active {
        map.insert(id.to_string(), prog);
    }

    Ok(map)
}

#[tauri::command]
async fn list_queues(manager: State<'_, AppManager>) -> Result<Vec<QueueRecord>, String> {
    manager.db().get_all_queues().map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_queue(
    manager: State<'_, AppManager>,
    name: String,
    max_concurrent: Option<usize>,
) -> Result<QueueRecord, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let id = format!("queue-{}", Uuid::new_v4().simple());
    let queue = QueueRecord {
        id,
        name,
        max_concurrent: max_concurrent.unwrap_or(2),
        speed_limit_kbps: 0,
        is_active: true,
        schedule_start: None,
        schedule_stop: None,
        created_at: now,
        on_complete_action: Some("none".to_string()),
    };
    manager.db().insert_or_update_queue(&queue).map_err(|e| e.to_string())?;
    Ok(queue)
}

#[tauri::command]
async fn update_queue(
    manager: State<'_, AppManager>,
    queue: QueueRecord,
) -> Result<bool, String> {
    manager.db().insert_or_update_queue(&queue).map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
async fn delete_queue(
    manager: State<'_, AppManager>,
    id: String,
) -> Result<bool, String> {
    manager.db().delete_queue(&id).map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
async fn start_queue(
    manager: State<'_, AppManager>,
    id: String,
) -> Result<bool, String> {
    manager.start_queue(&id).await.map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
async fn pause_queue(
    manager: State<'_, AppManager>,
    id: String,
) -> Result<bool, String> {
    manager.pause_queue(&id).map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
async fn move_task_to_queue(
    manager: State<'_, AppManager>,
    task_id: String,
    queue_id: String,
) -> Result<bool, String> {
    let id = Uuid::parse_str(&task_id).map_err(|e| e.to_string())?;
    manager.db().set_task_queue(id, &queue_id).map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
async fn abort_shutdown() -> Result<bool, String> {
    opendl_core::abort_system_shutdown().map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
async fn trigger_power_action(action: String) -> Result<bool, String> {
    opendl_core::trigger_power_action(&action).map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
async fn select_folder(default_path: Option<String>) -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        let initial = default_path.unwrap_or_else(|| {
            dirs_sys_or_default().unwrap_or_else(|| "C:\\".to_string())
        });
        let script = format!(
            "$f = (New-Object -ComObject Shell.Application).BrowseForFolder(0, 'Select Download Folder', 0, '{}'); if ($f) {{ $f.Self.Path }}",
            initial.replace('\'', "''")
        );
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .output()
            .map_err(|e| e.to_string())?;
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() {
            Ok(None)
        } else {
            Ok(Some(s))
        }
    }
    #[cfg(not(windows))]
    {
        Ok(default_path)
    }
}

#[tauri::command]
async fn open_path(path: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        std::process::Command::new("cmd")
            .raw_arg(format!("/C start \"\" \"{}\"", path))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn show_in_folder(path: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path))
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))]
    {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
async fn get_disk_free(path: Option<String>) -> Result<u64, String> {
    let check_path = path.unwrap_or_else(|| {
        if let Some(user_dirs) = dirs_sys_or_default() {
            user_dirs
        } else {
            "C:\\".to_string()
        }
    });

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = std::ffi::OsStr::new(&check_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut free_bytes_available: u64 = 0;
        let mut total_number_of_bytes: u64 = 0;
        let mut total_number_of_free_bytes: u64 = 0;
        unsafe {
            #[link(name = "kernel32")]
            extern "system" {
                fn GetDiskFreeSpaceExW(
                    lpDirectoryName: *const u16,
                    lpFreeBytesAvailableToCaller: *mut u64,
                    lpTotalNumberOfBytes: *mut u64,
                    lpTotalNumberOfFreeBytes: *mut u64,
                ) -> i32;
            }
            if GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free_bytes_available,
                &mut total_number_of_bytes,
                &mut total_number_of_free_bytes,
            ) != 0
            {
                return Ok(free_bytes_available);
            }
        }
        Err("Failed to query disk free space".to_string())
    }
    #[cfg(not(windows))]
    {
        Ok(100 * 1024 * 1024 * 1024)
    }
}

fn dirs_sys_or_default() -> Option<String> {
    if let Ok(profile) = std::env::var("USERPROFILE") {
        Some(PathBuf::from(profile).join("Downloads").to_string_lossy().to_string())
    } else {
        None
    }
}

#[tauri::command]
async fn minimize_window(window: tauri::Window) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}

#[tauri::command]
async fn toggle_maximize_window(window: tauri::Window) -> Result<bool, String> {
    let is_max = window.is_maximized().map_err(|e| e.to_string())?;
    if is_max {
        window.unmaximize().map_err(|e| e.to_string())?;
        Ok(false)
    } else {
        window.maximize().map_err(|e| e.to_string())?;
        Ok(true)
    }
}

#[tauri::command]
async fn close_window(window: tauri::Window) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}

#[tauri::command]
async fn is_window_maximized(window: tauri::Window) -> Result<bool, String> {
    window.is_maximized().map_err(|e| e.to_string())
}

fn get_db_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        PathBuf::from(appdata).join("OpenDL").join("opendl.db")
    } else {
        PathBuf::from("opendl.db")
    }
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("PANIC: {:?}", info);
        eprintln!("{}", msg);
        if let Ok(appdata) = std::env::var("APPDATA") {
            let path = PathBuf::from(appdata).join("OpenDL").join("crash.log");
            let _ = std::fs::write(path, &msg);
        }
    }));

    if let Err(e) = run_app() {
        let msg = format!("APPLICATION ERROR: {:?}", e);
        eprintln!("{}", msg);
        if let Ok(appdata) = std::env::var("APPDATA") {
            let path = PathBuf::from(appdata).join("OpenDL").join("crash.log");
            let _ = std::fs::write(path, &msg);
        }
    }
}

fn run_app() -> Result<()> {
    let log_file = if let Ok(appdata) = std::env::var("APPDATA") {
        let log_dir = PathBuf::from(appdata).join("OpenDL");
        let _ = std::fs::create_dir_all(&log_dir);
        std::fs::File::create(log_dir.join("app.log")).ok()
    } else {
        None
    };

    if let Some(file) = log_file {
        let _ = tracing_subscriber::fmt()
            .with_writer(std::sync::Mutex::new(file))
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
            )
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
            )
            .try_init();
    }

    info!("Starting OpenDL Desktop Application");

    let db_path = get_db_path();
    let db = Arc::new(Database::open(&db_path)?);
    let manager = Arc::new(DownloadManager::new(db.clone(), None));

    // Start background multi-queue scheduler
    manager.start_scheduler();

    // Spawn background Windows Named Pipe IPC server
    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);
    let manager_for_ipc = manager.clone();

    tauri::Builder::default()
        .manage(manager.clone())
        .setup(move |app| {
            // Ensure desktop windows are visible and focused
            for (_label, window) in app.webview_windows() {
                let _ = window.show();
                let _ = window.set_focus();
            }

            let handle = app.handle().clone();
            let mgr_ipc = manager_for_ipc.clone();

            tauri::async_runtime::spawn(async move {
                let handler = Arc::new(move |msg: IpcMessage| {
                    let h = handle.clone();
                    let mgr = mgr_ipc.clone();
                    async move {
                        match msg.action.as_str() {
                            "add_download_direct" => {
                                match mgr.add_from_ipc(msg, true).await {
                                    Ok(id) => IpcResponse::ok_with_id(id, "Task added and started"),
                                    Err(e) => IpcResponse::error(format!("Failed to add task: {}", e)),
                                }
                            }
                            "list_tasks" => {
                                match mgr.db().get_all_tasks() {
                                    Ok(tasks) => IpcResponse::ok_with_tasks(tasks),
                                    Err(e) => IpcResponse::error(format!("Failed to list tasks: {}", e)),
                                }
                            }
                            "pause_task" => {
                                if let Some(id) = msg.task_id {
                                    mgr.pause_task(id);
                                    IpcResponse::ok("Task paused")
                                } else {
                                    IpcResponse::error("Missing task_id")
                                }
                            }
                            "resume_task" => {
                                if let Some(id) = msg.task_id {
                                    match mgr.resume_task(id).await {
                                        Ok(_) => IpcResponse::ok("Task resumed"),
                                        Err(e) => IpcResponse::error(format!("Failed to resume task: {}", e)),
                                    }
                                } else {
                                    IpcResponse::error("Missing task_id")
                                }
                            }
                            "delete_task" => {
                                if let Some(id) = msg.task_id {
                                    match mgr.delete_task(id, false) {
                                        Ok(_) => IpcResponse::ok("Task deleted"),
                                        Err(e) => IpcResponse::error(format!("Failed to delete task: {}", e)),
                                    }
                                } else {
                                    IpcResponse::error("Missing task_id")
                                }
                            }
                            "list_queues" => {
                                match mgr.db().get_all_queues() {
                                    Ok(queues) => IpcResponse::ok_with_queues(queues),
                                    Err(e) => IpcResponse::error(format!("Failed to list queues: {}", e)),
                                }
                            }
                            "start_queue" => {
                                if let Some(qid) = msg.queue_id {
                                    let _ = mgr.start_queue(&qid).await;
                                    IpcResponse::ok("Queue started")
                                } else {
                                    IpcResponse::error("Missing queue_id")
                                }
                            }
                            "pause_queue" => {
                                if let Some(qid) = msg.queue_id {
                                    let _ = mgr.pause_queue(&qid);
                                    IpcResponse::ok("Queue paused")
                                } else {
                                    IpcResponse::error("Missing queue_id")
                                }
                            }
                            "abort_shutdown" => {
                                let _ = opendl_core::abort_system_shutdown();
                                IpcResponse::ok("Shutdown aborted")
                            }
                            "trigger_power" => {
                                let action = msg.filename.unwrap_or_else(|| "sleep".to_string());
                                let _ = opendl_core::trigger_power_action(&action);
                                IpcResponse::ok("Power action triggered")
                            }
                            _ => {
                                // Default interception: Prompt in desktop UI dialog
                                for (_label, window) in h.webview_windows() {
                                    let _ = window.unminimize();
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                    let _ = window.emit("browser_download_request", &msg);
                                }
                                IpcResponse::ok("Intercepted and prompted in OpenDL Desktop dialog")
                            }
                        }
                    }
                });

                if let Err(e) = IpcServer::run_server(DEFAULT_PIPE_NAME.to_string(), handler, shutdown_rx).await {
                    error!(error = %e, "IPC Server failed");
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            add_task,
            list_tasks,
            pause_task,
            resume_task,
            delete_task,
            get_all_progress,
            open_path,
            show_in_folder,
            get_disk_free,
            minimize_window,
            toggle_maximize_window,
            close_window,
            is_window_maximized,
            list_queues,
            create_queue,
            update_queue,
            delete_queue,
            start_queue,
            pause_queue,
            move_task_to_queue,
            select_folder,
            abort_shutdown,
            trigger_power_action,
        ])
        .run(tauri::generate_context!())
        .expect("Error while running Tauri application");

    let _ = shutdown_tx.send(());
    Ok(())
}

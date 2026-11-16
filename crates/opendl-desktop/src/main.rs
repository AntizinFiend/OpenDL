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
    #[allow(non_snake_case)]
    select_files: Option<String>,
    #[allow(non_snake_case)]
    selectFiles: Option<String>,
    #[allow(non_snake_case)]
    seed_ratio: Option<f64>,
    #[allow(non_snake_case)]
    seedRatio: Option<f64>,
    referrer: Option<String>,
    cookies: Option<String>,
    #[allow(non_snake_case)]
    user_agent: Option<String>,
    #[allow(non_snake_case)]
    userAgent: Option<String>,
    headers: Option<HashMap<String, String>>,
) -> Result<TaskRecord, String> {
    let should_start = start_now.unwrap_or(true);
    let mut hdrs = headers.unwrap_or_default();
    if let Some(sel) = select_files.or(selectFiles) {
        if !sel.trim().is_empty() {
            hdrs.insert("select-file".to_string(), sel);
        }
    }
    if let Some(ratio) = seed_ratio.or(seedRatio) {
        hdrs.insert("seed-ratio".to_string(), ratio.to_string());
    }

    let msg = IpcMessage {
        action: "add_download".to_string(),
        url: Some(url),
        filename,
        destination,
        referrer,
        cookies,
        user_agent: user_agent.or(userAgent),
        headers: if hdrs.is_empty() { None } else { Some(hdrs) },
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
async fn list_tasks(manager: State<'_, AppManager>, id: Option<String>) -> Result<Vec<TaskRecord>, String> {
    let _ = id;
    manager.db().get_all_tasks().map_err(|e| e.to_string())
}

#[tauri::command]
async fn pause_task(manager: State<'_, AppManager>, id: String) -> Result<bool, String> {
    if let Ok(task_id) = Uuid::parse_str(&id) {
        return Ok(manager.pause_task(task_id));
    }
    Ok(true)
}

#[tauri::command]
async fn resume_task(manager: State<'_, AppManager>, id: String) -> Result<bool, String> {
    if let Ok(task_id) = Uuid::parse_str(&id) {
        manager.resume_task(task_id).await.map_err(|e| e.to_string())?;
    }
    Ok(true)
}


#[tauri::command]
async fn delete_task(
    manager: State<'_, AppManager>,
    id: String,
    delete_file: Option<bool>,
    #[allow(non_snake_case)] deleteFile: Option<bool>,
) -> Result<bool, String> {
    let should_delete = delete_file.or(deleteFile).unwrap_or(false);
    if let Ok(task_id) = Uuid::parse_str(&id) {
        manager.delete_task(task_id, should_delete).map_err(|e| e.to_string())?;
    }
    Ok(true)
}

#[tauri::command]
async fn get_pending_intercepts(
    pending: State<'_, Arc<std::sync::Mutex<Vec<IpcMessage>>>>,
) -> Result<Vec<IpcMessage>, String> {
    let mut guard = pending.lock().map_err(|e| e.to_string())?;
    let items = guard.drain(..).collect();
    Ok(items)
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
                ..Default::default()
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
    let q_id = queue.id.clone();
    manager.db().insert_or_update_queue(&queue).map_err(|e| e.to_string())?;
    let _ = manager.process_queue(&q_id).await;
    Ok(true)
}

#[tauri::command]
async fn inspect_torrent_file(path: String) -> Result<opendl_core::TorrentInspectResult, String> {
    opendl_core::inspect_torrent_file(&path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn reorder_queue_tasks(
    manager: State<'_, AppManager>,
    queue_id: String,
    task_ids: Vec<String>,
) -> Result<bool, String> {
    let mut parsed = Vec::new();
    for id_str in task_ids {
        if let Ok(id) = Uuid::parse_str(&id_str) {
            parsed.push(id);
        }
    }
    manager
        .reorder_queue_and_reschedule(&queue_id, &parsed)
        .await
        .map_err(|e| e.to_string())?;
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
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let initial = default_path.unwrap_or_else(|| {
            dirs_sys_or_default().unwrap_or_else(|| "C:\\".to_string())
        });
        let script = format!(
            "$f = (New-Object -ComObject Shell.Application).BrowseForFolder(0, 'Select Download Folder', 0, '{}'); if ($f) {{ $f.Self.Path }}",
            initial.replace('\'', "''")
        );
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
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
async fn select_torrent_file() -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let script = "[System.Reflection.Assembly]::LoadWithPartialName('System.Windows.Forms') | Out-Null; $dlg = New-Object System.Windows.Forms.OpenFileDialog; $dlg.Filter = 'Torrent Files (*.torrent)|*.torrent|All Files (*.*)|*.*'; $dlg.Title = 'Select .torrent File'; if ($dlg.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { $dlg.FileName }";
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", script])
            .creation_flags(CREATE_NO_WINDOW)
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
        Ok(None)
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

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DriveInfo {
    pub letter: String,
    pub label: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub drive_type: String,
}

#[tauri::command]
async fn get_all_drives() -> Result<Vec<DriveInfo>, String> {
    #[cfg(windows)]
    {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;

        let mut drives = Vec::new();
        unsafe {
            #[link(name = "kernel32")]
            extern "system" {
                fn GetLogicalDriveStringsW(nBufferLength: u32, lpBuffer: *mut u16) -> u32;
                fn GetDriveTypeW(lpRootPathName: *const u16) -> u32;
                fn GetDiskFreeSpaceExW(
                    lpDirectoryName: *const u16,
                    lpFreeBytesAvailableToCaller: *mut u64,
                    lpTotalNumberOfBytes: *mut u64,
                    lpTotalNumberOfFreeBytes: *mut u64,
                ) -> i32;
                fn GetVolumeInformationW(
                    lpRootPathName: *const u16,
                    lpVolumeNameBuffer: *mut u16,
                    nVolumeNameSize: u32,
                    lpVolumeSerialNumber: *mut u32,
                    lpMaximumComponentLength: *mut u32,
                    lpFileSystemFlags: *mut u32,
                    lpFileSystemNameBuffer: *mut u16,
                    nFileSystemNameSize: u32,
                ) -> i32;
            }

            let mut buffer = [0u16; 512];
            let len = GetLogicalDriveStringsW(buffer.len() as u32, buffer.as_mut_ptr());
            if len > 0 && (len as usize) < buffer.len() {
                let mut start = 0;
                while start < (len as usize) {
                    let mut end = start;
                    while end < (len as usize) && buffer[end] != 0 {
                        end += 1;
                    }
                    if end > start {
                        let mut drive_w: Vec<u16> = buffer[start..end].to_vec();
                        drive_w.push(0);
                        let root_path = OsString::from_wide(&buffer[start..end]).to_string_lossy().to_string();
                        let dtype = GetDriveTypeW(drive_w.as_ptr());
                        let type_str = match dtype {
                            2 => "removable",
                            3 => "fixed",
                            4 => "network",
                            5 => "cdrom",
                            6 => "ramdisk",
                            _ => "other",
                        };

                        let mut free_bytes: u64 = 0;
                        let mut total_bytes: u64 = 0;
                        let mut total_free: u64 = 0;
                        let free_ok = GetDiskFreeSpaceExW(
                            drive_w.as_ptr(),
                            &mut free_bytes,
                            &mut total_bytes,
                            &mut total_free,
                        );

                        let mut vol_name = [0u16; 256];
                        let _ = GetVolumeInformationW(
                            drive_w.as_ptr(),
                            vol_name.as_mut_ptr(),
                            vol_name.len() as u32,
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                            0,
                        );
                        let label_len = vol_name.iter().position(|&c| c == 0).unwrap_or(0);
                        let label = if label_len > 0 {
                            OsString::from_wide(&vol_name[..label_len]).to_string_lossy().to_string()
                        } else {
                            format!("Local Disk ({})", root_path.trim_end_matches('\\'))
                        };

                        if free_ok != 0 && total_bytes > 0 {
                            drives.push(DriveInfo {
                                letter: root_path,
                                label,
                                total_bytes,
                                free_bytes,
                                drive_type: type_str.to_string(),
                            });
                        }
                    }
                    start = end + 1;
                }
            }
        }
        Ok(drives)
    }
    #[cfg(not(windows))]
    {
        Ok(vec![
            DriveInfo {
                letter: "/".to_string(),
                label: "Root Disk".to_string(),
                total_bytes: 500 * 1024 * 1024 * 1024,
                free_bytes: 120 * 1024 * 1024 * 1024,
                drive_type: "fixed".to_string(),
            }
        ])
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

#[tauri::command]
async fn start_dragging(window: tauri::Window) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
async fn probe_download(url: String) -> Result<opendl_engine::DownloadMetadata, String> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36")
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;

    let options = opendl_engine::DownloadOptions::new(&url, "");
    opendl_engine::probe::probe_url(&client, &options)
        .await
        .map_err(|e| e.to_string())
}

fn get_db_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        PathBuf::from(appdata).join("OpenDL").join("opendl.db")
    } else {
        PathBuf::from("opendl.db")
    }
}

#[cfg(windows)]
struct SingleInstanceMutex {
    handle: *mut std::ffi::c_void,
}

#[cfg(windows)]
impl SingleInstanceMutex {
    fn acquire() -> Option<Self> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let name: Vec<u16> = OsStr::new(r"Local\OpenDL_SingleInstance_Mutex")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            #[link(name = "kernel32")]
            extern "system" {
                fn CreateMutexW(
                    lpMutexAttributes: *mut std::ffi::c_void,
                    bInitialOwner: i32,
                    lpName: *const u16,
                ) -> *mut std::ffi::c_void;
                fn GetLastError() -> u32;
                fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
            }

            const ERROR_ALREADY_EXISTS: u32 = 183;
            let handle = CreateMutexW(std::ptr::null_mut(), 1, name.as_ptr());
            if handle.is_null() {
                return None;
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(handle);
                return None;
            }
            Some(Self { handle })
        }
    }
}

#[cfg(windows)]
impl Drop for SingleInstanceMutex {
    fn drop(&mut self) {
        unsafe {
            #[link(name = "kernel32")]
            extern "system" {
                fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
            }
            if !self.handle.is_null() {
                CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(windows)]
fn send_focus_to_existing_instance() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    let pipe_name: Vec<u16> = OsStr::new(DEFAULT_PIPE_NAME)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        #[link(name = "kernel32")]
        extern "system" {
            fn CreateFileW(
                lpFileName: *const u16,
                dwDesiredAccess: u32,
                dwShareMode: u32,
                lpSecurityAttributes: *mut std::ffi::c_void,
                dwCreationDisposition: u32,
                dwFlagsAndAttributes: u32,
                hTemplateFile: *mut std::ffi::c_void,
            ) -> *mut std::ffi::c_void;
            fn WriteFile(
                hFile: *mut std::ffi::c_void,
                lpBuffer: *const u8,
                nNumberOfBytesToWrite: u32,
                lpNumberOfBytesWritten: *mut u32,
                lpOverlapped: *mut std::ffi::c_void,
            ) -> i32;
            fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
        }

        const GENERIC_READ: u32 = 0x80000000;
        const GENERIC_WRITE: u32 = 0x40000000;
        const OPEN_EXISTING: u32 = 3;
        let invalid_handle = -1isize as *mut std::ffi::c_void;

        let handle = CreateFileW(
            pipe_name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );

        if handle != invalid_handle && !handle.is_null() {
            let msg = b"{\"action\":\"focus_window\"}\n";
            let mut written = 0u32;
            let _ = WriteFile(handle, msg.as_ptr(), msg.len() as u32, &mut written, std::ptr::null_mut());
            CloseHandle(handle);
        }
    }
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            format!("{:?}", info.payload())
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_string());

        let msg = format!("PANIC at [{}]: {}\n", location, payload);
        eprintln!("{}", msg);
        if let Ok(appdata) = std::env::var("APPDATA") {
            let path = PathBuf::from(appdata).join("OpenDL").join("crash.log");
            let _ = std::fs::write(path, &msg);
        }
    }));

    #[cfg(windows)]
    let _instance_mutex = match SingleInstanceMutex::acquire() {
        Some(m) => m,
        None => {
            // Application is already running in background/tray; focus existing window and exit cleanly
            send_focus_to_existing_instance();
            return;
        }
    };

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
    let pending_intercepts = Arc::new(std::sync::Mutex::new(Vec::new()));
    let pending_for_ipc = pending_intercepts.clone();

    tauri::Builder::default()
        .manage(manager.clone())
        .manage(pending_intercepts.clone())
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
                    let pending_queue = pending_for_ipc.clone();
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
                            "focus_window" => {
                                for (_label, window) in h.webview_windows() {
                                    let _ = window.unminimize();
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                                IpcResponse::ok("Window focused")
                            }
                            "move_task_queue" => {
                                if let (Some(id), Some(qid)) = (msg.task_id, msg.queue_id) {
                                    match mgr.db().set_task_queue(id, &qid) {
                                        Ok(_) => IpcResponse::ok("Task moved to queue"),
                                        Err(e) => IpcResponse::error(format!("Failed to move task: {}", e)),
                                    }
                                } else {
                                    IpcResponse::error("Missing task_id or queue_id")
                                }
                            }
                            _ => {
                                // Default interception: buffer in pending_intercepts and emit event
                                if let Ok(mut pending) = pending_queue.lock() {
                                    pending.push(msg.clone());
                                }
                                for (_label, window) in h.webview_windows() {
                                    let _ = window.unminimize();
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                    let _ = window.emit("browser_download_request", &msg);
                                    if let Ok(json_str) = serde_json::to_string(&msg) {
                                        let js = format!("if (window.__onBrowserDownloadRequest) {{ window.__onBrowserDownloadRequest({}); }}", json_str);
                                        let _ = window.eval(&js);
                                    }
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
            get_pending_intercepts,
            get_all_progress,
            open_path,
            show_in_folder,
            get_disk_free,
            minimize_window,
            toggle_maximize_window,
            close_window,
            is_window_maximized,
            start_dragging,
            probe_download,
            list_queues,
            create_queue,
            update_queue,
            delete_queue,
            start_queue,
            pause_queue,
            move_task_to_queue,
            select_folder,
            select_torrent_file,
            inspect_torrent_file,
            reorder_queue_tasks,
            get_all_drives,
            abort_shutdown,
            trigger_power_action,
        ])
        .on_window_event(|window, event| {
            info!(label = %window.label(), event = ?event, "Window event received");
        })
        .run(tauri::generate_context!())
        .expect("Error while running Tauri application");

    info!("Tauri run() finished and returned");
    let _ = shutdown_tx.send(());
    Ok(())
}

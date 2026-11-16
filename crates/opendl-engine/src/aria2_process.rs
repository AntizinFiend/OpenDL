use anyhow::{Context, Result};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::aria2_client::Aria2Client;

#[cfg(windows)]
mod win_job {
    use std::os::windows::io::RawHandle;
    use std::ptr::null_mut;

    #[repr(C)]
    struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    struct IO_COUNTERS {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        basic_limit_information: JOBOBJECT_BASIC_LIMIT_INFORMATION,
        io_info: IO_COUNTERS,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_limit: usize,
        peak_job_memory_limit: usize,
    }

    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(lpJobAttributes: *mut std::ffi::c_void, lpName: *const u16) -> RawHandle;
        fn SetInformationJobObject(
            hJob: RawHandle,
            JobObjectInformationClass: i32,
            lpJobObjectInformation: *const std::ffi::c_void,
            cbJobObjectInformationLength: u32,
        ) -> i32;
        fn AssignProcessToJobObject(hJob: RawHandle, hProcess: RawHandle) -> i32;
        fn CloseHandle(hObject: RawHandle) -> i32;
    }

    pub struct JobGuard {
        handle: RawHandle,
    }

    // Safety: JobGuard wraps a Windows HANDLE that can be closed from any thread.
    unsafe impl Send for JobGuard {}
    unsafe impl Sync for JobGuard {}

    impl JobGuard {
        pub fn create() -> Option<Self> {
            unsafe {
                let h_job = CreateJobObjectW(null_mut(), std::ptr::null());
                if h_job.is_null() || h_job == -1isize as RawHandle {
                    return None;
                }

                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

                let ok = SetInformationJobObject(
                    h_job,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                    &info as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );

                if ok == 0 {
                    CloseHandle(h_job);
                    return None;
                }

                Some(Self { handle: h_job })
            }
        }

        pub fn assign_process(&self, process_handle: RawHandle) -> bool {
            unsafe { AssignProcessToJobObject(self.handle, process_handle) != 0 }
        }
    }

    impl Drop for JobGuard {
        fn drop(&mut self) {
            unsafe {
                if !self.handle.is_null() && self.handle != -1isize as RawHandle {
                    CloseHandle(self.handle);
                }
            }
        }
    }
}

pub struct Aria2Process {
    child: Option<Child>,
    port: u16,
    secret: String,
    client: Aria2Client,
    #[cfg(windows)]
    _job_guard: Option<win_job::JobGuard>,
    stopped: Arc<AtomicBool>,
}

impl Aria2Process {
    /// Discover free port on local loopback.
    fn find_free_port() -> Result<u16> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .context("Failed to bind to 127.0.0.1:0 to discover free port")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        Ok(port)
    }

    /// Resolve aria2c executable location.
    pub fn locate_binary(custom_path: Option<&Path>) -> Result<PathBuf> {
        if let Some(p) = custom_path {
            if p.is_file() {
                return Ok(p.to_path_buf());
            }
        }

        let mut candidates = Vec::new();

        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(parent) = current_exe.parent() {
                candidates.push(parent.join("aria2c.exe"));
                candidates.push(parent.join("bin").join("aria2c.exe"));
                candidates.push(parent.join("aria2c"));
            }
        }

        if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let m_path = PathBuf::from(manifest_dir);
            candidates.push(m_path.join("bin").join("aria2c.exe"));
            if let Some(parent) = m_path.parent() {
                candidates.push(parent.join("opendl-desktop").join("bin").join("aria2c.exe"));
                if let Some(root) = parent.parent() {
                    candidates.push(root.join("crates").join("opendl-desktop").join("bin").join("aria2c.exe"));
                }
            }
        }

        // Relative paths from project workspace or runtime directory
        candidates.push(PathBuf::from("../opendl-desktop/bin/aria2c.exe"));
        candidates.push(PathBuf::from("crates/opendl-desktop/bin/aria2c.exe"));
        candidates.push(PathBuf::from("bin/aria2c.exe"));
        candidates.push(PathBuf::from("aria2c.exe"));
        candidates.push(PathBuf::from("aria2c"));

        for c in &candidates {
            if c.is_file() {
                return Ok(c.canonicalize().unwrap_or_else(|_| c.clone()));
            }
        }

        // Check PATH
        if let Ok(path_var) = std::env::var("PATH") {
            for entry in std::env::split_paths(&path_var) {
                let check_exe = entry.join("aria2c.exe");
                if check_exe.is_file() {
                    return Ok(check_exe);
                }
                let check_bin = entry.join("aria2c");
                if check_bin.is_file() {
                    return Ok(check_bin);
                }
            }
        }

        anyhow::bail!(
            "Could not locate aria2c executable. Checked candidates: {:?}",
            candidates
        )
    }

    /// Launch aria2c background daemon with Windows Job Object lifecycle protection.
    pub fn spawn(
        custom_binary: Option<&Path>,
        download_dir: Option<&Path>,
    ) -> Result<Self> {
        let binary_path = Self::locate_binary(custom_binary)?;
        let port = Self::find_free_port()?;
        let secret = Uuid::new_v4().simple().to_string();

        info!(
            binary = %binary_path.display(),
            port = port,
            "Spawning aria2c sidecar process"
        );

        let mut cmd = Command::new(&binary_path);
        cmd.arg("--enable-rpc=true")
            .arg("--rpc-listen-all=false")
            .arg(format!("--rpc-listen-port={}", port))
            .arg(format!("--rpc-secret={}", secret))
            .arg("--rpc-allow-origin-all=true")
            .arg("--max-connection-per-server=16")
            .arg("--split=16")
            .arg("--min-split-size=1M")
            .arg("--file-allocation=none")
            .arg("--no-file-allocation-limit=8M")
            .arg("--disk-cache=64M")
            .arg("--enable-dht=true")
            .arg("--enable-dht6=false")
            .arg("--bt-enable-lpd=true")
            .arg("--enable-peer-exchange=true")
            .arg("--listen-port=6881-6999")
            .arg("--dht-listen-port=6881-6999")
            .arg("--dht-entry-point=router.bittorrent.com:6881")
            .arg("--dht-entry-point=dht.transmissionbt.com:6881")
            .arg("--dht-entry-point=router.utorrent.com:6881")
            .arg("--bt-max-peers=60")
            .arg("--seed-ratio=1.0")
            .arg("--bt-save-metadata=true")
            .arg("--follow-torrent=mem")
            .arg("--bt-tracker=udp://tracker.opentrackr.org:1337/announce,udp://open.tracker.cl:1337/announce,udp://tracker.openbittorrent.com:6969/announce,udp://opentracker.i2p.rocks:6969/announce,udp://tracker.internetwarriors.net:1337/announce,udp://p4p.arenabg.com:1337/announce,udp://explodie.org:6969/announce")
            .arg("--conditional-get=true")
            .arg("--auto-file-renaming=false")
            .arg("--allow-overwrite=true")
            .arg("--disable-ipv6=true")
            .arg(format!("--user-agent={}", crate::resolver::DEFAULT_BROWSER_UA))
            .arg("--check-certificate=false")
            .arg("--http-accept-gzip=true")
            .arg("--remote-time=true")
            .arg("--max-tries=5")
            .arg("--retry-wait=2")
            .arg("--quiet=true");

        if let Some(dir) = download_dir {
            cmd.arg(format!("--dir={}", dir.display()));
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn aria2c from {}", binary_path.display()))?;

        #[cfg(windows)]
        let job_guard = {
            use std::os::windows::io::AsRawHandle;
            if let Some(job) = win_job::JobGuard::create() {
                let assigned = job.assign_process(child.as_raw_handle());
                if assigned {
                    info!("Successfully bound aria2c to Windows Job Object (JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)");
                } else {
                    warn!("Failed to assign aria2c process to Job Object");
                }
                Some(job)
            } else {
                warn!("Could not create Windows Job Object for aria2c");
                None
            }
        };

        let client = Aria2Client::new(port, secret.clone());

        Ok(Self {
            child: Some(child),
            port,
            secret,
            client,
            #[cfg(windows)]
            _job_guard: job_guard,
            stopped: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn secret(&self) -> &str {
        &self.secret
    }

    pub fn client(&self) -> Aria2Client {
        self.client.clone()
    }

    pub fn is_alive(&mut self) -> bool {
        if let Some(ref mut child) = self.child {
            match child.try_wait() {
                Ok(None) => true,
                Ok(Some(status)) => {
                    warn!(status = %status, "aria2c process has terminated");
                    false
                }
                Err(e) => {
                    error!(error = %e, "Failed to check aria2c process status");
                    false
                }
            }
        } else {
            false
        }
    }

    pub fn terminate(&mut self) {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return;
        }

        if let Some(mut child) = self.child.take() {
            info!("Shutting down aria2c process");
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Aria2Process {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_spawn_and_rpc() {
        let mut proc = Aria2Process::spawn(None, None).expect("Failed to spawn aria2c");
        assert!(proc.is_alive());
        let client = proc.client();

        // Give it a brief moment to open socket
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let version = client.get_version().await.expect("Failed to query get_version");
        assert!(version.contains("1.37") || !version.is_empty(), "Version: {}", version);

        proc.terminate();
        assert!(!proc.is_alive());
    }

    #[tokio::test]
    async fn test_aria2_download() {
        let mut proc = Aria2Process::spawn(None, None).expect("Failed to spawn aria2c");
        let client = proc.client();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let temp_dir = std::env::temp_dir().join(format!("opendl_test_{}", Uuid::new_v4().simple()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let gid = client
            .add_uri(
                &["https://raw.githubusercontent.com/curl/curl/master/README.md".to_string()],
                Some(&temp_dir),
                Some("test_readme.md"),
                &[],
                None,
                Some(4),
            )
            .await
            .expect("Failed to add URI");

        assert!(!gid.is_empty());

        let mut completed = false;
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let status = client.tell_status(&gid).await.expect("Failed to query tell_status");
            let progress = status.to_download_progress();
            if status.status == "complete" || progress.is_complete {
                completed = true;
                assert!(progress.downloaded_bytes > 0);
                assert!(!progress.segments.is_empty());
                break;
            }
        }

        assert!(completed, "Download should complete within 6 seconds");
        let target_file = temp_dir.join("test_readme.md");
        assert!(target_file.is_file(), "Downloaded file must exist");
        let _ = std::fs::remove_dir_all(&temp_dir);

        proc.terminate();
    }
}

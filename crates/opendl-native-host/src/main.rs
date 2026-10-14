mod framing;
mod install;
mod parser;

use anyhow::Result;
use clap::Parser;
use opendl_core::ipc::{IpcClient, IpcResponse, DEFAULT_PIPE_NAME};
use std::env;
use std::io::{stdin, stdout};
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;

use crate::framing::{read_native_message, write_native_message};
use crate::install::{install_native_manifests, uninstall_native_manifests};
use crate::parser::{parse_native_payload, ParsedPayload};

#[derive(Parser, Debug)]
#[command(name = "opendl-native-host", version = "0.1.0", about = "OpenDL Browser Native Messaging Host")]
struct Cli {
    /// Install native messaging manifests and browser registry entries
    #[arg(long)]
    install: bool,

    /// Optional extension ID to allow in native messaging host
    #[arg(long)]
    ext_id: Option<String>,

    /// Uninstall native messaging manifests and browser registry entries
    #[arg(long)]
    uninstall: bool,

    /// Trailing positional arguments passed by browser (e.g. extension origin, window handle)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    browser_args: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Avoid writing logging to stdout because stdout is reserved for browser native messaging!
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    let args = Cli::parse();

    if args.install {
        let mut ids = Vec::new();
        if let Some(id) = args.ext_id {
            ids.push(id);
        }
        return install_native_manifests(&ids);
    }

    if args.uninstall {
        return uninstall_native_manifests();
    }

    let mut stdin_lock = stdin().lock();
    let mut stdout_lock = stdout().lock();

    debug!("opendl-native-host started stdio loop");

    while let Ok(Some(raw_msg)) = read_native_message(&mut stdin_lock) {
        match parse_native_payload(&raw_msg) {
            Ok(ParsedPayload::PingAck(ack_bytes)) => {
                let _ = write_native_message(&mut stdout_lock, &ack_bytes);
            }
            Ok(ParsedPayload::Task(msg)) => {
                debug!(task = ?msg, "Forwarding task to OpenDL core IPC");

                // Send to Named Pipe IPC, with auto-launch if needed
                let resp = match forward_to_ipc(&msg).await {
                    Ok(r) => r,
                    Err(e) => IpcResponse::error(format!("Failed to contact OpenDL engine: {}", e)),
                };

                if let Ok(resp_bytes) = serde_json::to_vec(&resp) {
                    let _ = write_native_message(&mut stdout_lock, &resp_bytes);
                }
            }
            Ok(ParsedPayload::Ignored) => {
                let ack = serde_json::to_vec(&serde_json::json!({"status": "ignored"}))?;
                let _ = write_native_message(&mut stdout_lock, &ack);
            }
            Err(e) => {
                error!(error = %e, "Failed to parse native payload");
                let err_resp = serde_json::to_vec(&serde_json::json!({"status": "error", "error": e.to_string()}))?;
                let _ = write_native_message(&mut stdout_lock, &err_resp);
            }
        }
    }

    Ok(())
}

async fn forward_to_ipc(msg: &opendl_core::ipc::IpcMessage) -> Result<IpcResponse> {
    // Try sending directly
    if let Ok(resp) = IpcClient::send_request(DEFAULT_PIPE_NAME, msg).await {
        return Ok(resp);
    }

    // Pipe not available: try to launch desktop application
    try_launch_main_app();

    // Retry connecting for up to 3 seconds
    for _ in 0..12 {
        sleep(Duration::from_millis(250)).await;
        if let Ok(resp) = IpcClient::send_request(DEFAULT_PIPE_NAME, msg).await {
            return Ok(resp);
        }
    }

    anyhow::bail!("Timeout connecting to OpenDL core engine after auto-launch")
}

fn try_launch_main_app() {
    if let Ok(current_exe) = env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            let candidates = ["opendl-desktop.exe", "opendl.exe", "opendl-cli.exe"];
            for candidate in candidates {
                let p = exe_dir.join(candidate);
                if p.exists() {
                    info!(exe = ?p, "Auto-launching OpenDL core executable");
                    let _ = Command::new(p).spawn();
                    break;
                }
            }
        }
    }
}

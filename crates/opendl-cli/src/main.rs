use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use opendl_core::ipc::{IpcClient, IpcMessage, DEFAULT_PIPE_NAME};
use opendl_engine::{DownloadOptions, Downloader};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

mod mcp;

#[derive(Parser, Debug)]
#[command(
    name = "opendl",
    version = "0.1.0",
    about = "OpenDL Multi-Connection Download Accelerator & AI Grabber CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Direct URL to download standalone with progress bar
    #[arg(index = 1)]
    url: Option<String>,

    /// Destination file or directory path for standalone download
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Number of concurrent connections (1-32, default: 8)
    #[arg(short, long, default_value_t = 8)]
    connections: usize,

    /// Custom headers in "Key: Value" format
    #[arg(short = 'H', long = "header")]
    headers: Vec<String>,

    /// Minimum split threshold in megabytes (default: 1)
    #[arg(long, default_value_t = 1)]
    min_split_mb: u64,

    /// Speed limit in KB/s
    #[arg(short, long)]
    speed_limit_kb: Option<u64>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Add download to running OpenDL Desktop daemon
    Add {
        /// Download URL
        url: String,

        /// Custom filename
        #[arg(short, long)]
        filename: Option<String>,

        /// Target queue ID (e.g. "main", "night")
        #[arg(short, long)]
        queue: Option<String>,

        /// Do not prompt in desktop UI, start download directly
        #[arg(long, default_value_t = true)]
        start_now: bool,
    },

    /// List downloads in OpenDL Desktop
    List {
        /// Filter by queue ID
        #[arg(short, long)]
        queue: Option<String>,
    },

    /// Pause a download task in OpenDL Desktop
    Pause {
        /// Task UUID
        task_id: String,
    },

    /// Resume a download task in OpenDL Desktop
    Resume {
        /// Task UUID
        task_id: String,
    },

    /// Delete a download task in OpenDL Desktop
    Delete {
        /// Task UUID
        task_id: String,
    },

    /// Manage queues in OpenDL Desktop
    Queue {
        #[command(subcommand)]
        action: QueueCommands,
    },

    /// Abort pending system shutdown countdown
    AbortShutdown,

    /// Launch stdio Model Context Protocol (MCP) server for Antigravity AI agents
    Mcp,
}

#[derive(Subcommand, Debug)]
enum QueueCommands {
    /// List all queues
    List,
    /// Start processing a queue
    Start { queue_id: String },
    /// Pause a queue
    Pause { queue_id: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();

    // Check if subcommand was supplied
    if let Some(cmd) = args.command {
        match cmd {
            Commands::Add {
                url,
                filename,
                queue,
                start_now,
            } => {
                let msg = IpcMessage {
                    action: if start_now {
                        "add_download_direct".to_string()
                    } else {
                        "add_download".to_string()
                    },
                    url: Some(url),
                    filename,
                    destination: None,
                    referrer: None,
                    cookies: None,
                    user_agent: None,
                    headers: None,
                    task_id: None,
                    connections: None,
                    queue_id: queue,
                };

                let resp = IpcClient::send_request(DEFAULT_PIPE_NAME, &msg).await
                    .context("Failed to connect to OpenDL Desktop. Is the app running?")?;

                if resp.status == "ok" {
                    println!("[+] Successfully enqueued task in OpenDL! ID: {:?}", resp.task_id);
                } else {
                    eprintln!("[-] Error: {:?}", resp.message);
                }
            }

            Commands::List { queue } => {
                let msg = IpcMessage {
                    action: "list_tasks".to_string(),
                    url: None,
                    filename: None,
                    destination: None,
                    referrer: None,
                    cookies: None,
                    user_agent: None,
                    headers: None,
                    task_id: None,
                    connections: None,
                    queue_id: queue,
                };

                let resp = IpcClient::send_request(DEFAULT_PIPE_NAME, &msg).await
                    .context("Failed to connect to OpenDL Desktop. Is the app running?")?;

                if let Some(tasks) = resp.tasks {
                    println!("{:<38} {:<12} {:<10} {}", "TASK ID", "STATUS", "SIZE", "FILENAME");
                    println!("{:-<38} {:-<12} {:-<10} {:-<30}", "", "", "", "");
                    for t in tasks {
                        let size_str = t.total_size.map_or("Unknown".to_string(), |s| {
                            format!("{:.1} MB", s as f64 / (1024.0 * 1024.0))
                        });
                        println!("{:<38} {:<12} {:<10} {}", t.id, t.status.as_str(), size_str, t.filename);
                    }
                } else {
                    println!("No tasks returned.");
                }
            }

            Commands::Pause { task_id } => {
                let id = Uuid::parse_str(&task_id).context("Invalid UUID format")?;
                let msg = IpcMessage {
                    action: "pause_task".to_string(),
                    url: None,
                    filename: None,
                    destination: None,
                    referrer: None,
                    cookies: None,
                    user_agent: None,
                    headers: None,
                    task_id: Some(id),
                    connections: None,
                    queue_id: None,
                };
                let resp = IpcClient::send_request(DEFAULT_PIPE_NAME, &msg).await?;
                println!("[+] Result: {:?}", resp.message.unwrap_or_else(|| resp.status));
            }

            Commands::Resume { task_id } => {
                let id = Uuid::parse_str(&task_id).context("Invalid UUID format")?;
                let msg = IpcMessage {
                    action: "resume_task".to_string(),
                    url: None,
                    filename: None,
                    destination: None,
                    referrer: None,
                    cookies: None,
                    user_agent: None,
                    headers: None,
                    task_id: Some(id),
                    connections: None,
                    queue_id: None,
                };
                let resp = IpcClient::send_request(DEFAULT_PIPE_NAME, &msg).await?;
                println!("[+] Result: {:?}", resp.message.unwrap_or_else(|| resp.status));
            }

            Commands::Delete { task_id } => {
                let id = Uuid::parse_str(&task_id).context("Invalid UUID format")?;
                let msg = IpcMessage {
                    action: "delete_task".to_string(),
                    url: None,
                    filename: None,
                    destination: None,
                    referrer: None,
                    cookies: None,
                    user_agent: None,
                    headers: None,
                    task_id: Some(id),
                    connections: None,
                    queue_id: None,
                };
                let resp = IpcClient::send_request(DEFAULT_PIPE_NAME, &msg).await?;
                println!("[+] Result: {:?}", resp.message.unwrap_or_else(|| resp.status));
            }

            Commands::Queue { action } => match action {
                QueueCommands::List => {
                    let msg = IpcMessage {
                        action: "list_queues".to_string(),
                        url: None,
                        filename: None,
                        destination: None,
                        referrer: None,
                        cookies: None,
                        user_agent: None,
                        headers: None,
                        task_id: None,
                        connections: None,
                        queue_id: None,
                    };
                    let resp = IpcClient::send_request(DEFAULT_PIPE_NAME, &msg).await?;
                    if let Some(queues) = resp.queues {
                        println!("{:<12} {:<18} {:<10} {:<10} {:<10} {}", "QUEUE ID", "NAME", "MAX CONN", "SPEED LIM", "ACTIVE", "ACTION");
                        println!("{:-<12} {:-<18} {:-<10} {:-<10} {:-<10} {:-<10}", "", "", "", "", "", "");
                        for q in queues {
                            println!(
                                "{:<12} {:<18} {:<10} {:<10} {:<10} {}",
                                q.id,
                                q.name,
                                q.max_concurrent,
                                if q.speed_limit_kbps > 0 { format!("{} KB/s", q.speed_limit_kbps) } else { "Unlimited".to_string() },
                                q.is_active,
                                q.on_complete_action.unwrap_or_else(|| "none".to_string())
                            );
                        }
                    }
                }
                QueueCommands::Start { queue_id } => {
                    let msg = IpcMessage {
                        action: "start_queue".to_string(),
                        url: None,
                        filename: None,
                        destination: None,
                        referrer: None,
                        cookies: None,
                        user_agent: None,
                        headers: None,
                        task_id: None,
                        connections: None,
                        queue_id: Some(queue_id),
                    };
                    let resp = IpcClient::send_request(DEFAULT_PIPE_NAME, &msg).await?;
                    println!("[+] Result: {:?}", resp.message.unwrap_or_else(|| resp.status));
                }
                QueueCommands::Pause { queue_id } => {
                    let msg = IpcMessage {
                        action: "pause_queue".to_string(),
                        url: None,
                        filename: None,
                        destination: None,
                        referrer: None,
                        cookies: None,
                        user_agent: None,
                        headers: None,
                        task_id: None,
                        connections: None,
                        queue_id: Some(queue_id),
                    };
                    let resp = IpcClient::send_request(DEFAULT_PIPE_NAME, &msg).await?;
                    println!("[+] Result: {:?}", resp.message.unwrap_or_else(|| resp.status));
                }
            },

            Commands::AbortShutdown => {
                let msg = IpcMessage {
                    action: "abort_shutdown".to_string(),
                    url: None,
                    filename: None,
                    destination: None,
                    referrer: None,
                    cookies: None,
                    user_agent: None,
                    headers: None,
                    task_id: None,
                    connections: None,
                    queue_id: None,
                };
                let resp = IpcClient::send_request(DEFAULT_PIPE_NAME, &msg).await?;
                println!("[+] Result: {:?}", resp.message.unwrap_or_else(|| resp.status));
            }

            Commands::Mcp => {
                // Run stdio MCP server for Antigravity AI agents
                mcp::run_mcp_server().await?;
            }
        }
        return Ok(());
    }

    // Otherwise, standalone direct download mode
    let target_url = match args.url {
        Some(u) => u,
        None => {
            eprintln!("Error: Target URL or subcommand required. Run 'opendl --help' for usage.");
            std::process::exit(1);
        }
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    let mut header_map = HashMap::new();
    for h in args.headers {
        if let Some((k, v)) = h.split_once(':') {
            header_map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    let dest = args.output.unwrap_or_else(|| PathBuf::from("."));
    let mut options = DownloadOptions::new(&target_url, dest)
        .with_connections(args.connections);

    options.headers = header_map;
    options.min_split_threshold = args.min_split_mb * 1024 * 1024;
    if let Some(kb) = args.speed_limit_kb {
        options = options.with_speed_limit(kb * 1024);
    }

    println!("============================================================");
    println!(" OpenDL Multi-Connection Accelerator (opendl-cli)");
    println!(" Target URL:  {}", target_url);
    println!(" Connections: {}", args.connections);
    println!("============================================================");

    let downloader = Downloader::new(options)?;
    let (handle, join_handle) = downloader.start().await?;

    let meta = &handle.metadata;
    println!(" Filename:        {}", meta.suggested_filename);
    if let Some(len) = meta.content_length {
        println!(" Total Size:      {:.2} MB ({} bytes)", len as f64 / (1024.0 * 1024.0), len);
    } else {
        println!(" Total Size:      Unknown (streaming)");
    }
    println!(" Range Support:   {}", if meta.accepts_ranges { "Yes (Multi-Connection Dynamic Slicing)" } else { "No (Single Stream Fallback)" });
    println!("------------------------------------------------------------");

    let pb = if let Some(len) = meta.content_length {
        let bar = ProgressBar::new(len);
        bar.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({percent}%) | {msg}")
                .unwrap()
                .progress_chars("#>-"),
        );
        bar
    } else {
        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {bytes} downloaded | {msg}")
                .unwrap(),
        );
        bar
    };

    let mut rx = handle.progress_rx.clone();
    let handle_ctrlc = handle.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        println!("\n[!] Ctrl-C detected. Pausing and saving checkpoint...");
        handle_ctrlc.pause();
    });

    while rx.changed().await.is_ok() {
        let p = rx.borrow().clone();
        pb.set_position(p.downloaded_bytes);

        let speed_mb = p.speed_bytes_per_sec / (1024.0 * 1024.0);
        let active_workers = p.active_connections;
        let total_segments = p.segments.len();

        let msg = format!("{:.2} MB/s | Active Conns: {}/{}", speed_mb, active_workers, total_segments);
        pb.set_message(msg);

        if p.is_complete {
            break;
        }
    }

    join_handle.await.context("Download task panicked")??;
    pb.finish_with_message("Download complete!");

    println!("\n[+] Success: File downloaded successfully.");
    Ok(())
}

use anyhow::{Context, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use opendl_engine::{DownloadOptions, Downloader};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "opendl-cli", version = "0.1.0", about = "OpenDL Multi-Connection Download Accelerator")]
struct Cli {
    /// URL to download
    #[arg(required = true)]
    url: String,

    /// Destination file or directory path
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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    let args = Cli::parse();

    let mut header_map = HashMap::new();
    for h in args.headers {
        if let Some((k, v)) = h.split_once(':') {
            header_map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    let dest = args.output.unwrap_or_else(|| PathBuf::from("."));
    let mut options = DownloadOptions::new(&args.url, dest)
        .with_connections(args.connections);

    options.headers = header_map;
    options.min_split_threshold = args.min_split_mb * 1024 * 1024;
    if let Some(kb) = args.speed_limit_kb {
        options = options.with_speed_limit(kb * 1024);
    }

    println!("============================================================");
    println!(" OpenDL Dynamic Download Accelerator (opendl-cli)");
    println!(" Target URL: {}", args.url);
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

    // Monitor loop
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

    println!("\n[+] Success: File downloaded successfully without concatenation overhead.");
    Ok(())
}

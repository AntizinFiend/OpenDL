use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{info, warn};
use uuid::Uuid;

use crate::db::TaskRecord;

pub const DEFAULT_PIPE_NAME: &str = r"\\.\pipe\opendl_core";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage {
    pub action: String,
    pub url: Option<String>,
    pub filename: Option<String>,
    pub destination: Option<String>,
    pub referrer: Option<String>,
    pub cookies: Option<String>,
    pub user_agent: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub task_id: Option<Uuid>,
    pub connections: Option<usize>,
    pub queue_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcResponse {
    pub status: String,
    pub task_id: Option<Uuid>,
    pub message: Option<String>,
    pub tasks: Option<Vec<TaskRecord>>,
}

impl IpcResponse {
    pub fn ok(msg: impl Into<String>) -> Self {
        Self {
            status: "ok".to_string(),
            task_id: None,
            message: Some(msg.into()),
            tasks: None,
        }
    }

    pub fn ok_with_id(task_id: Uuid, msg: impl Into<String>) -> Self {
        Self {
            status: "ok".to_string(),
            task_id: Some(task_id),
            message: Some(msg.into()),
            tasks: None,
        }
    }

    pub fn ok_with_tasks(tasks: Vec<TaskRecord>) -> Self {
        Self {
            status: "ok".to_string(),
            task_id: None,
            message: None,
            tasks: Some(tasks),
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            status: "error".to_string(),
            task_id: None,
            message: Some(msg.into()),
            tasks: None,
        }
    }
}

pub struct IpcClient;

impl IpcClient {
    pub async fn send_request(pipe_name: &str, req: &IpcMessage) -> Result<IpcResponse> {
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ClientOptions;
            let client = ClientOptions::new()
                .open(pipe_name)
                .with_context(|| format!("Failed to open named pipe {}", pipe_name))?;

            let (reader, mut writer) = tokio::io::split(client);
            let mut line_reader = BufReader::new(reader);

            let mut json = serde_json::to_string(req)?;
            json.push('\n');
            writer.write_all(json.as_bytes()).await?;
            writer.flush().await?;

            let mut line = String::new();
            line_reader.read_line(&mut line).await?;
            let resp: IpcResponse = serde_json::from_str(&line)?;
            Ok(resp)
        }

        #[cfg(not(windows))]
        {
            anyhow::bail!("Named pipes are only supported on Windows in this build");
        }
    }
}

pub struct IpcServer;

impl IpcServer {
    pub async fn run_server<F, Fut>(
        pipe_name: String,
        handler: Arc<F>,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<()>
    where
        F: Fn(IpcMessage) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = IpcResponse> + Send + 'static,
    {
        info!(pipe_name = %pipe_name, "Starting Windows Named Pipe IPC server");

        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ServerOptions;

            let mut server = ServerOptions::new()
                .first_pipe_instance(true)
                .create(&pipe_name)
                .with_context(|| format!("Failed to create named pipe instance {}", pipe_name))?;

            loop {
                tokio::select! {
                    connect_res = server.connect() => {
                        match connect_res {
                            Ok(()) => {
                                let connected_pipe = server;
                                // Immediately create next pipe instance for concurrent incoming clients
                                server = ServerOptions::new()
                                    .first_pipe_instance(false)
                                    .create(&pipe_name)?;

                                let h = handler.clone();
                                tokio::spawn(async move {
                                    let (reader, mut writer) = tokio::io::split(connected_pipe);
                                    let mut line_reader = BufReader::new(reader);
                                    let mut line = String::new();

                                    if let Ok(n) = line_reader.read_line(&mut line).await {
                                        if n > 0 {
                                            let resp = match serde_json::from_str::<IpcMessage>(&line) {
                                                Ok(msg) => h(msg).await,
                                                Err(e) => IpcResponse::error(format!("Invalid JSON: {}", e)),
                                            };

                                            if let Ok(mut resp_json) = serde_json::to_string(&resp) {
                                                resp_json.push('\n');
                                                let _ = writer.write_all(resp_json.as_bytes()).await;
                                                let _ = writer.flush().await;
                                            }
                                        }
                                    }
                                });
                            }
                            Err(e) => {
                                warn!(error = %e, "Error accepting named pipe client");
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Named pipe IPC server shutting down");
                        break;
                    }
                }
            }

            Ok(())
        }

        #[cfg(not(windows))]
        {
            warn!("Named pipe IPC server is only supported on Windows");
            Ok(())
        }
    }
}

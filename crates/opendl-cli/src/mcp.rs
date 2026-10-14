use anyhow::{Context, Result};
use opendl_core::ipc::{IpcClient, IpcMessage, DEFAULT_PIPE_NAME};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct McpRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct McpResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

pub async fn run_mcp_server() -> Result<()> {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        debug!(input = %trimmed, "Received MCP message");

        if let Ok(req) = serde_json::from_str::<McpRequest>(trimmed) {
            let resp = handle_mcp_request(req).await;
            if let Some(r) = resp {
                let mut out = serde_json::to_string(&r)?;
                out.push('\n');
                stdout.write_all(out.as_bytes()).await?;
                stdout.flush().await?;
            }
        } else {
            error!(raw = %trimmed, "Malformed MCP request");
        }

        line.clear();
    }

    Ok(())
}

async fn handle_mcp_request(req: McpRequest) -> Option<McpResponse> {
    let id = req.id;

    match req.method.as_str() {
        "initialize" => {
            let res = json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "opendl-mcp",
                    "version": "0.1.0"
                }
            });
            Some(McpResponse {
                jsonrpc: "2.0",
                id,
                result: Some(res),
                error: None,
            })
        }
        "notifications/initialized" => None,
        "ping" => Some(McpResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({})),
            error: None,
        }),
        "tools/list" => {
            let tools = json!({
                "tools": [
                    {
                        "name": "opendl_add_download",
                        "description": "Enqueue a new accelerated download into OpenDL with multi-connection segmentation",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "url": { "type": "string", "description": "Direct download URL" },
                                "filename": { "type": "string", "description": "Custom destination filename (optional)" },
                                "queue_id": { "type": "string", "description": "Target queue ID (e.g. 'main', 'night', optional)" },
                                "start_now": { "type": "boolean", "description": "Whether to start immediately (default: true)" },
                                "referrer": { "type": "string", "description": "HTTP Referer header (optional)" },
                                "cookies": { "type": "string", "description": "Cookie string (optional)" }
                            },
                            "required": ["url"]
                        }
                    },
                    {
                        "name": "opendl_batch_enqueue",
                        "description": "Batch enqueue multiple URLs into an OpenDL queue (Site Grabber replacement). Perfect for queuing lists of PDFs, videos, datasets, or software packages discovered during web browsing.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "urls": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "description": "List of URLs to download"
                                },
                                "queue_id": { "type": "string", "description": "Target queue ID (optional, defaults to 'main')" },
                                "start_now": { "type": "boolean", "description": "Whether to start downloading immediately (default: true)" }
                            },
                            "required": ["urls"]
                        }
                    },
                    {
                        "name": "opendl_list_downloads",
                        "description": "List all active, queued, paused, completed, and failed downloads in OpenDL",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "queue_id": { "type": "string", "description": "Filter by queue ID (optional)" }
                            }
                        }
                    },
                    {
                        "name": "opendl_pause_download",
                        "description": "Pause an active download by task ID",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "task_id": { "type": "string", "description": "UUID of the task" }
                            },
                            "required": ["task_id"]
                        }
                    },
                    {
                        "name": "opendl_resume_download",
                        "description": "Resume a paused download by task ID",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "task_id": { "type": "string", "description": "UUID of the task" }
                            },
                            "required": ["task_id"]
                        }
                    },
                    {
                        "name": "opendl_delete_download",
                        "description": "Delete a download task from OpenDL",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "task_id": { "type": "string", "description": "UUID of the task" }
                            },
                            "required": ["task_id"]
                        }
                    },
                    {
                        "name": "opendl_list_queues",
                        "description": "List all download queues and their status, concurrency limits, and scheduled times",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "opendl_start_queue",
                        "description": "Activate and start processing downloads in a queue",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "queue_id": { "type": "string", "description": "Queue identifier" }
                            },
                            "required": ["queue_id"]
                        }
                    },
                    {
                        "name": "opendl_pause_queue",
                        "description": "Pause all active downloads in a queue",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "queue_id": { "type": "string", "description": "Queue identifier" }
                            },
                            "required": ["queue_id"]
                        }
                    },
                    {
                        "name": "opendl_abort_shutdown",
                        "description": "Abort a pending system shutdown if a queue completion countdown was triggered",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    }
                ]
            });
            Some(McpResponse {
                jsonrpc: "2.0",
                id,
                result: Some(tools),
                error: None,
            })
        }
        "tools/call" => {
            let res = execute_tool_call(req.params.unwrap_or(json!({}))).await;
            match res {
                Ok(val) => Some(McpResponse {
                    jsonrpc: "2.0",
                    id,
                    result: Some(json!({
                        "content": [
                            {
                                "type": "text",
                                "text": val
                            }
                        ]
                    })),
                    error: None,
                }),
                Err(e) => Some(McpResponse {
                    jsonrpc: "2.0",
                    id,
                    result: Some(json!({
                        "isError": true,
                        "content": [
                            {
                                "type": "text",
                                "text": format!("Error executing tool: {}", e)
                            }
                        ]
                    })),
                    error: None,
                }),
            }
        }
        _ => Some(McpResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(json!({
                "code": -32601,
                "message": format!("Method not found: {}", req.method)
            })),
        }),
    }
}

async fn execute_tool_call(params: Value) -> Result<String> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing tool name in params"))?;

    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    match name {
        "opendl_add_download" => {
            let url = args.get("url").and_then(|v| v.as_str()).context("Missing url")?;
            let filename = args.get("filename").and_then(|v| v.as_str()).map(|s| s.to_string());
            let queue_id = args.get("queue_id").and_then(|v| v.as_str()).map(|s| s.to_string());
            let start_now = args.get("start_now").and_then(|v| v.as_bool()).unwrap_or(true);
            let referrer = args.get("referrer").and_then(|v| v.as_str()).map(|s| s.to_string());
            let cookies = args.get("cookies").and_then(|v| v.as_str()).map(|s| s.to_string());

            let msg = IpcMessage {
                action: if start_now { "add_download_direct".to_string() } else { "add_download".to_string() },
                url: Some(url.to_string()),
                filename,
                destination: None,
                referrer,
                cookies,
                user_agent: None,
                headers: None,
                task_id: None,
                connections: None,
                queue_id,
            };

            let resp = IpcClient::send_request(DEFAULT_PIPE_NAME, &msg).await
                .context("Failed to connect to OpenDL Desktop IPC. Ensure OpenDL Desktop is running.")?;

            if resp.status == "ok" {
                Ok(format!("Download successfully enqueued in OpenDL. Task ID: {:?}", resp.task_id))
            } else {
                anyhow::bail!("OpenDL returned error: {:?}", resp.message)
            }
        }

        "opendl_batch_enqueue" => {
            let urls = args.get("urls").and_then(|v| v.as_array()).context("Missing urls array")?;
            let queue_id = args.get("queue_id").and_then(|v| v.as_str()).map(|s| s.to_string());
            let start_now = args.get("start_now").and_then(|v| v.as_bool()).unwrap_or(true);

            let mut added_count = 0;
            let mut failed_count = 0;

            for u in urls {
                if let Some(url_str) = u.as_str() {
                    let msg = IpcMessage {
                        action: if start_now { "add_download_direct".to_string() } else { "add_download".to_string() },
                        url: Some(url_str.to_string()),
                        filename: None,
                        destination: None,
                        referrer: None,
                        cookies: None,
                        user_agent: None,
                        headers: None,
                        task_id: None,
                        connections: None,
                        queue_id: queue_id.clone(),
                    };

                    if let Ok(resp) = IpcClient::send_request(DEFAULT_PIPE_NAME, &msg).await {
                        if resp.status == "ok" {
                            added_count += 1;
                        } else {
                            failed_count += 1;
                        }
                    } else {
                        failed_count += 1;
                    }
                }
            }

            Ok(format!("Site Grabber Batch Complete: Enqueued {} files successfully ({} failed). Target queue: {:?}", added_count, failed_count, queue_id.unwrap_or_else(|| "main".to_string())))
        }

        "opendl_list_downloads" => {
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
                queue_id: args.get("queue_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
            };

            let resp = IpcClient::send_request(DEFAULT_PIPE_NAME, &msg).await
                .context("Failed to connect to OpenDL Desktop IPC")?;

            if let Some(tasks) = resp.tasks {
                let summary = serde_json::to_string_pretty(&tasks)?;
                Ok(summary)
            } else {
                Ok("No tasks found in OpenDL.".to_string())
            }
        }

        "opendl_pause_download" => {
            let tid_str = args.get("task_id").and_then(|v| v.as_str()).context("Missing task_id")?;
            let task_id = Uuid::parse_str(tid_str)?;

            let msg = IpcMessage {
                action: "pause_task".to_string(),
                url: None,
                filename: None,
                destination: None,
                referrer: None,
                cookies: None,
                user_agent: None,
                headers: None,
                task_id: Some(task_id),
                connections: None,
                queue_id: None,
            };

            let resp = IpcClient::send_request(DEFAULT_PIPE_NAME, &msg).await?;
            Ok(resp.message.unwrap_or_else(|| "Paused".to_string()))
        }

        "opendl_resume_download" => {
            let tid_str = args.get("task_id").and_then(|v| v.as_str()).context("Missing task_id")?;
            let task_id = Uuid::parse_str(tid_str)?;

            let msg = IpcMessage {
                action: "resume_task".to_string(),
                url: None,
                filename: None,
                destination: None,
                referrer: None,
                cookies: None,
                user_agent: None,
                headers: None,
                task_id: Some(task_id),
                connections: None,
                queue_id: None,
            };

            let resp = IpcClient::send_request(DEFAULT_PIPE_NAME, &msg).await?;
            Ok(resp.message.unwrap_or_else(|| "Resumed".to_string()))
        }

        "opendl_delete_download" => {
            let tid_str = args.get("task_id").and_then(|v| v.as_str()).context("Missing task_id")?;
            let task_id = Uuid::parse_str(tid_str)?;

            let msg = IpcMessage {
                action: "delete_task".to_string(),
                url: None,
                filename: None,
                destination: None,
                referrer: None,
                cookies: None,
                user_agent: None,
                headers: None,
                task_id: Some(task_id),
                connections: None,
                queue_id: None,
            };

            let resp = IpcClient::send_request(DEFAULT_PIPE_NAME, &msg).await?;
            Ok(resp.message.unwrap_or_else(|| "Deleted".to_string()))
        }

        "opendl_list_queues" => {
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
                Ok(serde_json::to_string_pretty(&queues)?)
            } else {
                Ok("No queues found".to_string())
            }
        }

        "opendl_start_queue" => {
            let queue_id = args.get("queue_id").and_then(|v| v.as_str()).context("Missing queue_id")?;
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
                queue_id: Some(queue_id.to_string()),
            };

            let resp = IpcClient::send_request(DEFAULT_PIPE_NAME, &msg).await?;
            Ok(resp.message.unwrap_or_else(|| "Queue started".to_string()))
        }

        "opendl_pause_queue" => {
            let queue_id = args.get("queue_id").and_then(|v| v.as_str()).context("Missing queue_id")?;
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
                queue_id: Some(queue_id.to_string()),
            };

            let resp = IpcClient::send_request(DEFAULT_PIPE_NAME, &msg).await?;
            Ok(resp.message.unwrap_or_else(|| "Queue paused".to_string()))
        }

        "opendl_abort_shutdown" => {
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
            Ok(resp.message.unwrap_or_else(|| "Shutdown countdown aborted".to_string()))
        }

        _ => anyhow::bail!("Unknown tool name: {}", name),
    }
}

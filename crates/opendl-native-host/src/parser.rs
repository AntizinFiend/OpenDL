use anyhow::Result;
use opendl_core::ipc::IpcMessage;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug)]
pub enum ParsedPayload {
    Task(IpcMessage),
    PingAck(Vec<u8>),
    Ignored,
}

pub fn parse_native_payload(raw: &[u8]) -> Result<ParsedPayload> {
    let text = String::from_utf8_lossy(raw);
    let trimmed = text.trim();

    // Explicitly reject proprietary or legacy wire protocol strings
    if trimmed.starts_with("MSG#") || trimmed.starts_with("\"MSG#") {
        return Ok(ParsedPayload::Ignored);
    }

    // OpenDL uses standard JSON messages: {"action": "download", ...}
    let effective_str = if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        if let Ok(unquoted) = serde_json::from_str::<String>(trimmed) {
            unquoted
        } else {
            trimmed.to_string()
        }
    } else {
        trimmed.to_string()
    };

    if let Ok(val) = serde_json::from_str::<Value>(&effective_str) {
        if let Some(obj) = val.as_object() {
            let action = obj
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("add_download")
                .to_string();

            if action == "ping" {
                return Ok(ParsedPayload::PingAck(
                    serde_json::to_vec(&serde_json::json!({"status": "ok", "pong": true}))?,
                ));
            }

            let url = obj.get("url").and_then(|v| v.as_str()).map(|s| s.to_string());
            let filename = obj.get("filename").and_then(|v| v.as_str()).map(|s| s.to_string());
            let destination = obj.get("destination").and_then(|v| v.as_str()).map(|s| s.to_string());
            let referrer = obj.get("referrer").and_then(|v| v.as_str()).map(|s| s.to_string());
            let cookies = obj.get("cookies").and_then(|v| v.as_str()).map(|s| s.to_string());
            let user_agent = obj.get("user_agent").and_then(|v| v.as_str()).map(|s| s.to_string());

            let mut headers = HashMap::new();
            if let Some(h_obj) = obj.get("headers").and_then(|v| v.as_object()) {
                for (k, v) in h_obj {
                    if let Some(s) = v.as_str() {
                        headers.insert(k.clone(), s.to_string());
                    }
                }
            }

            return Ok(ParsedPayload::Task(IpcMessage {
                action,
                url,
                filename,
                destination,
                referrer,
                cookies,
                user_agent,
                headers: if headers.is_empty() { None } else { Some(headers) },
                task_id: None,
                connections: obj.get("connections").and_then(|v| v.as_u64()).map(|n| n as usize),
            }));
        }
    }

    Ok(ParsedPayload::Ignored)
}

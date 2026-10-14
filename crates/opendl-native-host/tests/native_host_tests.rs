#[path = "../src/framing.rs"]
mod framing;
#[path = "../src/parser.rs"]
mod parser;

use framing::{read_native_message, write_native_message};
use parser::{parse_native_payload, ParsedPayload};
use std::io::Cursor;

#[test]
fn test_length_prefixed_framing() {
    let message = b"{\"action\":\"ping\"}";
    let mut buffer = Vec::new();

    write_native_message(&mut buffer, message).unwrap();
    assert_eq!(buffer.len(), 4 + message.len());

    let mut cursor = Cursor::new(buffer);
    let read_back = read_native_message(&mut cursor).unwrap().unwrap();
    assert_eq!(read_back, message);
}

#[test]
fn test_ping_parsing() {
    let json_bytes = b"{\"action\":\"ping\"}";
    let parsed = parse_native_payload(json_bytes).unwrap();
    match parsed {
        ParsedPayload::PingAck(ack) => {
            let s = String::from_utf8(ack).unwrap();
            assert!(s.contains("\"pong\":true"));
        }
        _ => panic!("Expected PingAck"),
    }
}

#[test]
fn test_legacy_wire_format_ignored() {
    // Legacy / proprietary wire format frames must be completely ignored
    let handshake_frame = b"MSG#1#2#5#1024[:112:89:1031],112=Chrome";
    let parsed = parse_native_payload(handshake_frame).unwrap();
    assert!(matches!(parsed, ParsedPayload::Ignored));

    let download_frame = br#"MSG#4#13#1#129,6="https://downloads.example.com/installer.iso",4="installer.iso""#;
    let parsed_dl = parse_native_payload(download_frame).unwrap();
    assert!(matches!(parsed_dl, ParsedPayload::Ignored));
}

#[test]
fn test_json_download_parsing() {
    let json_bytes = br#"{
        "action": "download",
        "url": "https://example.com/file.zip",
        "filename": "custom_file.zip",
        "referrer": "https://example.com/",
        "connections": 16
    }"#;

    let parsed = parse_native_payload(json_bytes).unwrap();
    match parsed {
        ParsedPayload::Task(msg) => {
            assert_eq!(msg.action, "download");
            assert_eq!(msg.url, Some("https://example.com/file.zip".to_string()));
            assert_eq!(msg.filename, Some("custom_file.zip".to_string()));
            assert_eq!(msg.connections, Some(16));
        }
        _ => panic!("Expected Task parsed payload"),
    }
}

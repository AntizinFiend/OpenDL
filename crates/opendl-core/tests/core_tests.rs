use opendl_core::db::{Database, TaskRecord, TaskStatus};
use opendl_core::ipc::{IpcClient, IpcMessage, IpcResponse, IpcServer};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

#[test]
fn test_database_category_classification() {
    let db = Database::in_memory().unwrap();

    assert_eq!(db.classify_category("archive.7z"), "Compressed");
    assert_eq!(db.classify_category("document.pdf"), "Documents");
    assert_eq!(db.classify_category("song.mp3"), "Music");
    assert_eq!(db.classify_category("setup.exe"), "Programs");
    assert_eq!(db.classify_category("movie.mkv"), "Video");
    assert_eq!(db.classify_category("unknown.xyz"), "General");
}

#[test]
fn test_database_crud_operations() {
    let db = Database::in_memory().unwrap();
    let task_id = Uuid::new_v4();

    let record = TaskRecord {
        id: task_id,
        url: "https://example.com/file.zip".to_string(),
        filename: "file.zip".to_string(),
        destination: PathBuf::from("D:\\Downloads\\file.zip"),
        total_size: Some(10485760),
        downloaded_bytes: 0,
        status: TaskStatus::Queued,
        category: "Compressed".to_string(),
        etag: Some("\"etag123\"".to_string()),
        created_at: 1000,
        updated_at: 1000,
    };

    db.insert_task(&record).unwrap();

    let fetched = db.get_task(task_id).unwrap().expect("Task should exist");
    assert_eq!(fetched.filename, "file.zip");
    assert_eq!(fetched.status, TaskStatus::Queued);

    db.update_task_progress(task_id, 5000000, TaskStatus::Downloading).unwrap();
    let updated = db.get_task(task_id).unwrap().unwrap();
    assert_eq!(updated.downloaded_bytes, 5000000);
    assert_eq!(updated.status, TaskStatus::Downloading);

    let all = db.get_all_tasks().unwrap();
    assert_eq!(all.len(), 1);

    db.delete_task(task_id).unwrap();
    assert!(db.get_task(task_id).unwrap().is_none());
}

#[tokio::test]
async fn test_named_pipe_ipc_roundtrip() {
    #[cfg(windows)]
    {
        let pipe_name = format!(r"\\.\pipe\opendl_test_{}", Uuid::new_v4());
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);

        let handler = Arc::new(|msg: IpcMessage| async move {
            if msg.action == "ping" {
                IpcResponse::ok("pong")
            } else {
                IpcResponse::ok_with_id(Uuid::new_v4(), "Queued")
            }
        });

        let p_name = pipe_name.clone();
        tokio::spawn(async move {
            let _ = IpcServer::run_server(p_name, handler, shutdown_rx).await;
        });

        // Give server a moment to bind
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let req = IpcMessage {
            action: "ping".to_string(),
            url: None,
            filename: None,
            destination: None,
            referrer: None,
            cookies: None,
            user_agent: None,
            headers: None,
            task_id: None,
            connections: None,
        };

        let resp = IpcClient::send_request(&pipe_name, &req).await.unwrap();
        assert_eq!(resp.status, "ok");
        assert_eq!(resp.message, Some("pong".to_string()));

        let _ = shutdown_tx.send(());
    }
}

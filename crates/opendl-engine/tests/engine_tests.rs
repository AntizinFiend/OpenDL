use opendl_engine::probe::extract_filename;
use opendl_engine::segment_allocator::SegmentAllocator;
use opendl_engine::storage::Storage;
use opendl_engine::types::DEFAULT_MIN_SPLIT_THRESHOLD;

#[test]
fn test_filename_extraction() {
    // Content-Disposition header
    let disp1 = r#"attachment; filename="archive.zip""#;
    assert_eq!(extract_filename(Some(disp1), None, "https://example.com/download"), "archive.zip");

    // UTF-8 encoded Content-Disposition
    let disp2 = "attachment; filename*=UTF-8''document%20test.pdf";
    assert_eq!(extract_filename(Some(disp2), None, "https://example.com/download"), "document test.pdf");

    // Fallback to URL path
    assert_eq!(extract_filename(None, None, "https://example.com/path/to/my_file.tar.gz?token=123"), "my_file.tar.gz");

    // Fallback on empty path
    assert_eq!(extract_filename(None, None, "https://example.com/"), "download.bin");

    // Video stream with videoplayback path and video/mp4 MIME
    assert_eq!(
        extract_filename(None, Some("video/mp4"), "https://rr1---sn-4g5edn6e.googlevideo.com/videoplayback?expire=123&mime=video%2Fmp4"),
        "video.mp4"
    );

    // Video stream with title and webm MIME
    assert_eq!(
        extract_filename(None, Some("video/webm"), "https://example.com/videoplayback?title=My+Sample+Video&mime=video/webm"),
        "My Sample Video.webm"
    );

    // Query parameter filename
    assert_eq!(
        extract_filename(None, None, "https://example.com/stream.php?filename=presentation.mp4"),
        "presentation.mp4"
    );
}

#[test]
fn test_segment_allocator_initialization() {
    let total_size = 100 * 1024 * 1024; // 100 MB
    let allocator = SegmentAllocator::new_initial(total_size, 8, DEFAULT_MIN_SPLIT_THRESHOLD);
    let initial = allocator.initial_segments();

    assert_eq!(initial.len(), 8);
    assert_eq!(initial[0].start, 0);
    assert_eq!(initial[7].end.load(std::sync::atomic::Ordering::Relaxed), total_size - 1);
}

#[test]
fn test_dynamic_segment_splitting() {
    let total_size = 10 * 1024 * 1024; // 10 MB
    let min_split = 1024 * 1024; // 1 MB
    let allocator = SegmentAllocator::new_initial(total_size, 2, min_split);
    let initial = allocator.initial_segments();

    // Mark segment 0 as active
    initial[0].is_active.store(true, std::sync::atomic::Ordering::Relaxed);
    // Downloaded 1MB of 5MB
    initial[0].current.store(1024 * 1024, std::sync::atomic::Ordering::Relaxed);

    // Segment 1 finished
    initial[1].is_completed.store(true, std::sync::atomic::Ordering::Relaxed);
    initial[1].is_active.store(false, std::sync::atomic::Ordering::Relaxed);

    // Worker 1 is idle, calls try_split_largest()
    let split_seg = allocator.try_split_largest();
    assert!(split_seg.is_some(), "Should split segment 0 because remaining bytes >= 2x min_split");

    let new_seg = split_seg.unwrap();
    assert_eq!(new_seg.id, 2);
    // Original end was ~5MB - 1, split should be approximately halfway between 1MB and 5MB
    let seg0_new_end = initial[0].end.load(std::sync::atomic::Ordering::Relaxed);
    assert!(seg0_new_end < 5 * 1024 * 1024);
    assert_eq!(new_seg.start, seg0_new_end + 1);
}

#[test]
fn test_storage_direct_writes() {
    let temp_dir = std::env::temp_dir().join(format!("opendl_test_{}", uuid::Uuid::new_v4()));
    let file_path = temp_dir.join("test_file.bin");

    let total_size = 1024 * 1024; // 1 MB
    let storage = Storage::create_or_open(&file_path, Some(total_size)).unwrap();

    let chunk1 = b"Hello from Segment 0";
    let chunk2 = b"Hello from Segment 1 at offset 500000";

    storage.write_at(0, chunk1).unwrap();
    storage.write_at(500_000, chunk2).unwrap();
    storage.sync().unwrap();

    let read_back = std::fs::read(&file_path).unwrap();
    assert_eq!(read_back.len(), total_size as usize);
    assert_eq!(&read_back[0..chunk1.len()], chunk1);
    assert_eq!(&read_back[500_000..500_000 + chunk2.len()], chunk2);

    let _ = std::fs::remove_dir_all(temp_dir);
}

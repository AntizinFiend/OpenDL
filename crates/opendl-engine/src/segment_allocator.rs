use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tracing::info;

use crate::types::SegmentProgress;

#[derive(Debug)]
pub struct Segment {
    pub id: usize,
    pub start: u64,
    pub current: Arc<AtomicU64>,
    pub end: Arc<AtomicU64>,
    pub is_active: Arc<AtomicBool>,
    pub is_completed: Arc<AtomicBool>,
}

impl Segment {
    pub fn new(id: usize, start: u64, end: u64) -> Self {
        Self {
            id,
            start,
            current: Arc::new(AtomicU64::new(start)),
            end: Arc::new(AtomicU64::new(end)),
            is_active: Arc::new(AtomicBool::new(false)),
            is_completed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn to_progress(&self) -> SegmentProgress {
        let curr = self.current.load(Ordering::Relaxed);
        let e = self.end.load(Ordering::Relaxed);
        let completed = self.is_completed.load(Ordering::Relaxed);
        SegmentProgress {
            id: self.id,
            start: self.start,
            current: curr,
            end: e,
            is_active: self.is_active.load(Ordering::Relaxed),
            is_completed: completed || (curr > e),
        }
    }
}

pub struct SegmentAllocator {
    min_split_threshold: u64,
    segments: Mutex<Vec<Arc<Segment>>>,
    next_id: Mutex<usize>,
}

impl SegmentAllocator {
    pub fn new_initial(total_size: u64, max_connections: usize, min_split_threshold: u64) -> Self {
        let min_threshold = min_split_threshold.max(256 * 1024); // at least 256KB
        let num_connections = if total_size < min_threshold * 2 {
            1
        } else {
            let possible = (total_size / min_threshold).clamp(1, max_connections as u64) as usize;
            possible.min(max_connections)
        };

        let chunk_size = total_size / (num_connections as u64);
        let mut segments = Vec::with_capacity(num_connections);

        for i in 0..num_connections {
            let start = i as u64 * chunk_size;
            let end = if i == num_connections - 1 {
                total_size - 1
            } else {
                (i as u64 + 1) * chunk_size - 1
            };
            segments.push(Arc::new(Segment::new(i, start, end)));
        }

        info!(
            total_size = total_size,
            initial_segments = num_connections,
            "SegmentAllocator initialized with uniform partitions"
        );

        Self {
            min_split_threshold: min_threshold,
            segments: Mutex::new(segments),
            next_id: Mutex::new(num_connections),
        }
    }

    pub fn from_existing(segments: Vec<SegmentProgress>, min_split_threshold: u64) -> Self {
        let max_id = segments.iter().map(|s| s.id).max().unwrap_or(0);
        let segs: Vec<Arc<Segment>> = segments
            .into_iter()
            .map(|sp| {
                let seg = Arc::new(Segment::new(sp.id, sp.start, sp.end));
                seg.current.store(sp.current, Ordering::Relaxed);
                seg.is_completed.store(sp.is_completed, Ordering::Relaxed);
                seg
            })
            .collect();

        Self {
            min_split_threshold: min_split_threshold.max(256 * 1024),
            segments: Mutex::new(segs),
            next_id: Mutex::new(max_id + 1),
        }
    }

    pub fn initial_segments(&self) -> Vec<Arc<Segment>> {
        let segs = self.segments.lock();
        segs.clone()
    }

    pub fn try_split_largest(&self) -> Option<Arc<Segment>> {
        let mut segs = self.segments.lock();

        let mut best_index = None;
        let mut max_remaining = 0u64;

        for (idx, seg) in segs.iter().enumerate() {
            if !seg.is_active.load(Ordering::Relaxed) || seg.is_completed.load(Ordering::Relaxed) {
                continue;
            }

            let curr = seg.current.load(Ordering::Relaxed);
            let end = seg.end.load(Ordering::Relaxed);

            if end > curr {
                let remaining = end - curr;
                if remaining > max_remaining {
                    max_remaining = remaining;
                    best_index = Some(idx);
                }
            }
        }

        // Only split if remaining uncompleted bytes are at least 2x min_split_threshold
        if max_remaining >= 2 * self.min_split_threshold {
            if let Some(idx) = best_index {
                let (target_id, curr, orig_end) = {
                    let target = &segs[idx];
                    (target.id, target.current.load(Ordering::Relaxed), target.end.load(Ordering::Relaxed))
                };

                if orig_end > curr && (orig_end - curr) >= 2 * self.min_split_threshold {
                    let split_point = curr + (orig_end - curr) / 2;

                    // Shrink target's end to split_point
                    segs[idx].end.store(split_point, Ordering::SeqCst);

                    let new_id = {
                        let mut next_id = self.next_id.lock();
                        let id = *next_id;
                        *next_id += 1;
                        id
                    };

                    let new_segment = Arc::new(Segment::new(new_id, split_point + 1, orig_end));
                    segs.push(new_segment.clone());

                    info!(
                        split_from_id = target_id,
                        new_id = new_id,
                        new_start = split_point + 1,
                        new_end = orig_end,
                        "Dynamically split segment"
                    );

                    return Some(new_segment);
                }
            }
        }

        None
    }

    pub fn progress_snapshot(&self) -> (u64, Vec<SegmentProgress>, usize) {
        let segs = self.segments.lock();
        let mut total_downloaded = 0u64;
        let mut active_count = 0usize;
        let mut list = Vec::with_capacity(segs.len());

        for seg in segs.iter() {
            let p = seg.to_progress();
            if p.is_active {
                active_count += 1;
            }
            total_downloaded += p.downloaded_bytes();
            list.push(p);
        }

        (total_downloaded, list, active_count)
    }

    pub fn is_all_completed(&self) -> bool {
        let segs = self.segments.lock();
        segs.iter().all(|s| {
            let curr = s.current.load(Ordering::Relaxed);
            let end = s.end.load(Ordering::Relaxed);
            s.is_completed.load(Ordering::Relaxed) || (curr > end)
        })
    }
}

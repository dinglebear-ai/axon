use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn drain_joins_existing_and_late_workers() {
    let workers = DetachedWorkerRegistry::default();
    let existing = Arc::new(AtomicBool::new(false));
    let done = Arc::clone(&existing);
    workers.track(std::thread::spawn(move || {
        done.store(true, Ordering::Release)
    }));
    workers.drain();
    assert!(existing.load(Ordering::Acquire));
    let late = Arc::new(AtomicBool::new(false));
    let done = Arc::clone(&late);
    workers.track(std::thread::spawn(move || {
        done.store(true, Ordering::Release)
    }));
    assert!(late.load(Ordering::Acquire));
}

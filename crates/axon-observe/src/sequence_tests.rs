use super::*;
use axon_api::source::JobId;

#[test]
fn sequences_start_at_one_and_strictly_increase() {
    let registry = SequenceRegistry::new();
    let job = JobId(uuid::Uuid::new_v4());

    let mut prev = 0;
    for expected in 1..=100 {
        let next = registry.next(job);
        assert_eq!(next, expected);
        assert!(next > prev, "sequence must strictly increase");
        prev = next;
    }
    assert_eq!(registry.last(job), Some(100));
}

#[test]
fn distinct_jobs_have_independent_counters() {
    let registry = SequenceRegistry::new();
    let a = JobId(uuid::Uuid::new_v4());
    let b = JobId(uuid::Uuid::new_v4());

    assert_eq!(registry.next(a), 1);
    assert_eq!(registry.next(a), 2);
    assert_eq!(registry.next(b), 1);
    assert_eq!(registry.next(a), 3);
    assert_eq!(registry.next(b), 2);

    assert_eq!(registry.last(a), Some(3));
    assert_eq!(registry.last(b), Some(2));
    assert_eq!(registry.stream_count(), 2);
}

#[test]
fn last_is_none_before_any_issue() {
    let registry = SequenceRegistry::new();
    let job = JobId(uuid::Uuid::new_v4());
    assert_eq!(registry.last(job), None);
    assert_eq!(registry.stream_count(), 0);
}

#[test]
fn shared_registry_is_monotonic_across_concurrent_emitters() {
    use std::sync::Arc;
    use std::thread;

    let registry = Arc::new(SequenceRegistry::new());
    let job = JobId(uuid::Uuid::new_v4());
    let mut handles = Vec::new();
    for _ in 0..8 {
        let registry = Arc::clone(&registry);
        handles.push(thread::spawn(move || {
            let mut seen = Vec::new();
            for _ in 0..50 {
                seen.push(registry.next(job));
            }
            seen
        }));
    }

    let mut all: Vec<u64> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    all.sort_unstable();
    // 8 threads * 50 = 400 unique sequences, 1..=400, no gaps or dupes.
    assert_eq!(all.len(), 400);
    assert_eq!(*all.first().unwrap(), 1);
    assert_eq!(*all.last().unwrap(), 400);
    for window in all.windows(2) {
        assert!(
            window[1] > window[0],
            "no duplicate sequences under contention"
        );
    }
}

use super::*;

#[test]
fn computes_dangling_stored_ids() {
    let stored = vec!["a", "b", "c", "d"];
    let live = vec!["b", "d"];
    let orphans = orphaned_ids(&stored, &live);
    assert_eq!(orphans, vec!["a", "c"]);
}

#[test]
fn deduplicates_and_sorts_orphans() {
    let stored = vec!["c", "a", "c", "a", "b"];
    let live: Vec<&str> = vec![];
    let orphans = orphaned_ids(&stored, &live);
    assert_eq!(orphans, vec!["a", "b", "c"]);
}

#[test]
fn no_orphans_when_all_live() {
    let stored = vec![1, 2, 3];
    let live = vec![1, 2, 3, 4];
    assert!(orphaned_ids(&stored, &live).is_empty());
    assert!(!has_orphans(&stored, &live));
}

#[test]
fn has_orphans_true_when_dangling() {
    assert!(has_orphans(&[1, 2, 9], &[1, 2]));
}

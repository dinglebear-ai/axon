use super::*;

// fnv64_url — hash stability and collision properties

#[test]
fn fnv64_url_empty_string_returns_offset_basis() {
    // FNV-1a: empty input returns the offset basis unchanged.
    assert_eq!(fnv64_url(""), 14695981039346656037u64);
}

#[test]
fn fnv64_url_same_input_returns_same_hash() {
    let url = "https://example.com/docs/page";
    assert_eq!(fnv64_url(url), fnv64_url(url));
}

#[test]
fn fnv64_url_different_inputs_return_different_hashes() {
    let a = fnv64_url("https://example.com/page-a");
    let b = fnv64_url("https://example.com/page-b");
    assert_ne!(a, b, "distinct URLs must produce distinct hashes");
}

#[test]
fn fnv64_url_single_char_difference_changes_hash() {
    let a = fnv64_url("https://example.com/a");
    let b = fnv64_url("https://example.com/b");
    assert_ne!(a, b);
}

// select_stale_ids — selection logic unit tests (T-M4)

#[test]
fn select_stale_ids_empty_returns_empty() {
    let result = select_stale_ids(vec![]);
    assert!(result.is_empty());
}

#[test]
fn select_stale_ids_single_record_returns_empty() {
    // One record = no duplicates; nothing to delete.
    let result = select_stale_ids(vec![(
        "id-1".to_string(),
        "2024-01-01T00:00:00Z".to_string(),
    )]);
    assert!(result.is_empty(), "single record must not be deleted");
}

#[test]
fn select_stale_ids_two_records_keeps_newest() {
    let records = vec![
        ("id-old".to_string(), "2024-01-01T00:00:00Z".to_string()),
        ("id-new".to_string(), "2024-06-01T00:00:00Z".to_string()),
    ];
    let to_delete = select_stale_ids(records);
    assert_eq!(to_delete, vec!["id-old"], "older record must be deleted");
}

#[test]
fn select_stale_ids_three_records_deletes_two_oldest() {
    let records = vec![
        ("id-b".to_string(), "2024-03-01T00:00:00Z".to_string()),
        ("id-a".to_string(), "2024-01-01T00:00:00Z".to_string()),
        ("id-c".to_string(), "2024-06-01T00:00:00Z".to_string()),
    ];
    let mut to_delete = select_stale_ids(records);
    to_delete.sort(); // order is unspecified for the delete set
    assert_eq!(
        to_delete,
        vec!["id-a", "id-b"],
        "two older records must be deleted"
    );
}

#[test]
fn select_stale_ids_same_scraped_at_keeps_one() {
    // When timestamps are identical, exactly one record survives regardless of which.
    let records = vec![
        ("id-x".to_string(), "2024-06-01T00:00:00Z".to_string()),
        ("id-y".to_string(), "2024-06-01T00:00:00Z".to_string()),
    ];
    let to_delete = select_stale_ids(records);
    assert_eq!(to_delete.len(), 1, "one of the two must be deleted");
}

#[test]
fn select_stale_ids_result_excludes_survivor() {
    let records = vec![
        ("id-old".to_string(), "2024-01-01T00:00:00Z".to_string()),
        ("id-new".to_string(), "2024-12-31T23:59:59Z".to_string()),
    ];
    let to_delete = select_stale_ids(records);
    assert!(
        !to_delete.contains(&"id-new".to_string()),
        "newest record must not appear in delete list"
    );
}

#[test]
fn select_stale_ids_input_order_independent() {
    let forward = vec![
        ("id-1".to_string(), "2024-01-01T00:00:00Z".to_string()),
        ("id-2".to_string(), "2024-06-01T00:00:00Z".to_string()),
        ("id-3".to_string(), "2024-03-01T00:00:00Z".to_string()),
    ];
    let reverse = vec![
        ("id-3".to_string(), "2024-03-01T00:00:00Z".to_string()),
        ("id-2".to_string(), "2024-06-01T00:00:00Z".to_string()),
        ("id-1".to_string(), "2024-01-01T00:00:00Z".to_string()),
    ];
    let mut a = select_stale_ids(forward);
    let mut b = select_stale_ids(reverse);
    a.sort();
    b.sort();
    assert_eq!(a, b, "result must be independent of input order");
}

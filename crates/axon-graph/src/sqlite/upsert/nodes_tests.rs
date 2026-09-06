use super::*;

#[test]
fn node_write_batches_stay_under_sqlite_variable_limit() {
    let bind_count = std::hint::black_box(NODE_WRITE_BIND_COUNT);
    let batch_size = std::hint::black_box(NODE_WRITE_BATCH_SIZE);
    assert_eq!(bind_count, 11);
    assert!(batch_size * bind_count <= 999);
    assert!((batch_size + 1) * bind_count > 999);
}

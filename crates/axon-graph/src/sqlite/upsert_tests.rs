use super::*;

#[test]
fn edge_statement_batches_stay_below_sqlite_bind_limit() {
    assert_eq!(edge_read_batch_sizes(1_001), vec![900, 101]);
    assert_eq!(edge_write_batch_sizes(201), vec![100, 100, 1]);
    const {
        assert!(EDGE_WRITE_BATCH_SIZE * EDGE_WRITE_BINDS_PER_ROW <= SQLITE_SAFE_BIND_LIMIT);
    }
}

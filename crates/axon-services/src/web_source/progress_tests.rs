use super::*;

#[test]
fn downstream_totals_stay_unknown_until_the_final_web_batch() {
    let mut progress = WebPipelineProgress::new(2);

    assert_eq!(progress.fetch_start().items_total, Some(2));
    progress.acquired(1, 1);
    let first_normalized = progress.normalized(1, false);
    let first_prepared = progress.prepared(1, 350, false);
    let first_batched = progress.batched(350);
    let first_embedded = progress.embedded(350);
    let first_vectorized = progress.vectorized(250, false);

    for counts in [
        first_normalized,
        first_prepared,
        first_batched,
        first_embedded,
        first_vectorized,
    ] {
        assert_eq!(counts.documents_total, None);
        assert_eq!(counts.chunks_total, None);
    }

    progress.acquired(1, 1);
    let final_normalized = progress.normalized(1, true);
    let final_prepared = progress.prepared(1, 350, true);
    let final_batched = progress.batched(350);
    let final_embedded = progress.embedded(350);
    let final_vectorized = progress.vectorized(250, true);
    let final_upserted = progress.upserted(500);

    assert_eq!(final_normalized.documents_total, Some(2));
    assert_eq!(final_prepared.documents_total, Some(2));
    assert_eq!(final_prepared.chunks_total, Some(700));
    assert_eq!(final_batched.chunks_total, Some(700));
    assert_eq!(final_embedded.chunks_total, Some(700));
    assert_eq!(final_vectorized.chunks_total, Some(500));
    assert_eq!(final_upserted.chunks_total, Some(500));
    assert_eq!(final_upserted.chunks_done, 500);
}

#[test]
fn web_progress_counts_never_exceed_known_totals() {
    let mut progress = WebPipelineProgress::new(1);
    progress.acquired(4, 4);
    let normalized = progress.normalized(4, true);
    let prepared = progress.prepared(4, 10, true);
    let vectorized = progress.vectorized(12, true);
    let upserted = progress.upserted(20);

    assert_eq!(normalized.items_done, 1);
    assert_eq!(normalized.documents_done, 1);
    assert_eq!(prepared.documents_done, 1);
    assert_eq!(prepared.chunks_done, 10);
    assert_eq!(vectorized.chunks_total, Some(12));
    assert_eq!(vectorized.chunks_done, 12);
    assert_eq!(upserted.chunks_done, 12);
}

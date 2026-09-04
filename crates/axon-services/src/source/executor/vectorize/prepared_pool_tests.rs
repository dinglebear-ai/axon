use super::*;

fn status(chunks: u32, points: u32) -> DocumentStatus {
    DocumentStatus {
        document_id: DocumentId::new("split-document"),
        source_id: SourceId::new("source"),
        source_item_key: SourceItemKey::new("item"),
        generation: Some(SourceGenerationId::new("1")),
        status: DocumentLifecycleStatus::Vectorized,
        updated_at: timestamp(),
        chunk_count: chunks,
        vector_point_count: points,
        error: None,
        cleanup_status: None,
    }
}

#[test]
fn touched_checkpoint_is_cumulative_for_a_document_spanning_pools() {
    let mut cumulative = HashMap::new();
    assert_eq!(
        merge_and_collect_touched(&mut cumulative, &[status(2, 2)])[0].vector_point_count,
        2
    );
    let second = merge_and_collect_touched(&mut cumulative, &[status(3, 3)]);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].chunk_count, 5);
    assert_eq!(second[0].vector_point_count, 5);
}

use super::*;

fn prepared_document(chunk_count: usize) -> PreparedDocument {
    let source_id = SourceId::new("src-window-test");
    let item_key = SourceItemKey::new("item-window-test");
    let document_id = DocumentId::new("doc-window-test");
    PreparedDocument {
        document_id: document_id.clone(),
        source_id,
        source_item_key: item_key,
        generation: SourceGenerationId::new("1"),
        canonical_uri: "memory://window-test".to_string(),
        prepare_version: "test".to_string(),
        chunking_profile: "test".to_string(),
        chunking_method: "test".to_string(),
        chunks: (0..chunk_count)
            .map(|index| PreparedChunk {
                chunk_id: ChunkId::new(format!("chunk-{index}")),
                chunk_key: format!("chunk-{index}"),
                document_id: document_id.clone(),
                chunk_index: index as u32,
                content: format!("chunk {index}"),
                content_hash: format!("hash-{index}"),
                embedding_text: None,
                chunk_locator: ChunkLocator {
                    canonical_uri: "memory://window-test".to_string(),
                    path: None,
                    heading_path: Vec::new(),
                    symbol: None,
                    range: empty_range(),
                },
                source_range: empty_range(),
                content_kind: ContentKind::Markdown,
                title: None,
                graph_refs: Vec::new(),
                parent_chunk_id: None,
                previous_chunk_id: None,
                next_chunk_id: None,
                metadata: MetadataMap::new(),
            })
            .collect(),
        metadata: MetadataMap(
            [
                ("source_family".to_string(), serde_json::json!("web")),
                ("source_kind".to_string(), serde_json::json!("web")),
                ("source_adapter".to_string(), serde_json::json!("web")),
                ("source_scope".to_string(), serde_json::json!("page")),
                (
                    "item_canonical_uri".to_string(),
                    serde_json::json!("memory://window-test"),
                ),
            ]
            .into_iter()
            .collect(),
        ),
        cleanup_keys: Vec::new(),
        graph_refs: Vec::new(),
        parse_facts: Vec::new(),
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    }
}

fn empty_range() -> SourceRange {
    SourceRange {
        line_start: None,
        line_end: None,
        byte_start: None,
        byte_end: None,
        char_start: None,
        char_end: None,
        time_start_ms: None,
        time_end_ms: None,
        dom_selector: None,
        json_pointer: None,
        yaml_path: None,
        xml_xpath: None,
        csv_row: None,
        session_turn_id: None,
        turn_start: None,
        turn_end: None,
    }
}

#[test]
fn oversized_document_is_split_into_bounded_chunk_windows() {
    let chunk_count = CHUNK_BATCH_SIZE * 2 + 1;
    let batches = chunk_batches(vec![prepared_document(chunk_count)]);

    assert_eq!(batches.len(), 3);
    assert!(batches.iter().all(|batch| {
        batch
            .iter()
            .map(|document| document.chunks.len())
            .sum::<usize>()
            <= CHUNK_BATCH_SIZE
    }));
    assert_eq!(
        batches
            .iter()
            .flat_map(|batch| batch.iter())
            .map(|document| document.chunks.len())
            .sum::<usize>(),
        chunk_count
    );
}

#[test]
fn split_windows_merge_back_to_one_document_status_and_total_chunk_count() {
    let chunk_count = CHUNK_BATCH_SIZE + 7;
    let mut merged = VectorizeResult::default();
    for window in split_oversized_document(prepared_document(chunk_count)) {
        merge_vectorize_result(
            &mut merged,
            statuses_only(vec![window], DocumentLifecycleStatus::Prepared),
        );
    }

    assert_eq!(merged.documents_prepared, 1);
    assert_eq!(merged.chunks_prepared, chunk_count as u64);
    assert_eq!(merged.document_statuses.len(), 1);
    assert_eq!(merged.document_statuses[0].chunk_count, chunk_count as u32);
}

#[test]
fn redaction_skips_reduce_document_vector_point_count() {
    let mut document = axon_vectors::testing::test_prepared_document();
    document.chunks[1].content = "API_KEY=abc123".to_string();
    let document_id = document.document_id.clone();
    let embeddings =
        axon_vectors::testing::test_embedding_result_for(&document, "text-embedding-test", 3);
    let build = point_batch(
        axon_vectors::testing::test_collection_spec(3),
        std::slice::from_ref(&document),
        &embeddings,
    )
    .expect("build vector points");

    assert_eq!(build.batch.points.len(), 1);
    assert_eq!(build.skipped_redaction, 1);
    assert_eq!(build.points_by_document.get(&document_id), Some(&1));

    let mut result = statuses_only(vec![document], DocumentLifecycleStatus::Vectorized);
    apply_vector_point_counts(&mut result.document_statuses, &build.points_by_document);
    assert_eq!(result.document_statuses[0].chunk_count, 2);
    assert_eq!(result.document_statuses[0].vector_point_count, 1);
}

#[test]
fn successful_short_upsert_is_rejected() {
    let error = validate_upsert_counts(3, 3, 2).expect_err("short write must fail");
    assert!(error.to_string().contains("vector upsert short write"));
    validate_upsert_counts(3, 3, 3).expect("complete write");
}

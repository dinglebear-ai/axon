use super::batching::{chunk_batches, split_oversized_document};
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

fn source_document(index: usize) -> SourceDocument {
    SourceDocument {
        document_id: DocumentId::new(format!("source-doc-{index}")),
        source_id: SourceId::new("source-batch-test"),
        source_item_key: SourceItemKey::new(format!("item-{index}")),
        canonical_uri: format!("memory://source-batch-test/{index}"),
        content_kind: ContentKind::Markdown,
        content: ContentRef::InlineText {
            text: format!("document {index}"),
        },
        metadata: MetadataMap::new(),
        title: None,
        language: None,
        path: None,
        mime_type: Some("text/markdown".to_string()),
        structured_payload: None,
        artifact_id: None,
        chunk_hints: Vec::new(),
        parser_hints: Vec::new(),
    }
}

#[test]
fn generation_document_batches_obey_resolved_runtime_batch_size() {
    let batches =
        generation_document_batches((0..7).map(source_document).collect(), 3).collect::<Vec<_>>();

    assert_eq!(batches.iter().map(Vec::len).collect::<Vec<_>>(), [3, 3, 1]);
    assert_eq!(
        batches
            .into_iter()
            .flatten()
            .map(|document| document.document_id.0)
            .collect::<Vec<_>>(),
        (0..7)
            .map(|index| format!("source-doc-{index}"))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn document_status_writes_are_bounded_ordered_and_complete() {
    let ledger = axon_ledger::store::FakeLedgerStore::new();
    let source_id = SourceId::new("status-batch-source");
    ledger
        .upsert_source(SourceSummary {
            source_id: source_id.clone(),
            canonical_uri: "memory://status-batch-source".to_string(),
            display_name: "status batch source".to_string(),
            source_kind: SourceKind::Memory,
            adapter: AdapterRef {
                name: "memory".to_string(),
                version: "test".to_string(),
            },
            authority: AuthorityLevel::UserPinned,
            status: LifecycleStatus::Running,
            counts: SourceCounts {
                items_total: 0,
                items_changed: 0,
                documents_total: 0,
                chunks_total: 0,
                vector_points_total: 0,
                bytes_total: 0,
            },
            created_at: timestamp(),
            updated_at: timestamp(),
            tags: Vec::new(),
            watch_id: None,
            graph_node_ids: Vec::new(),
            last_job_id: None,
            last_refreshed_at: None,
            user_label: None,
        })
        .await
        .expect("seed source");
    let statuses = (0..5)
        .map(|index| DocumentStatus {
            document_id: DocumentId::new(format!("status-doc-{index}")),
            source_id: source_id.clone(),
            source_item_key: SourceItemKey::new(format!("item-{index}")),
            generation: Some(SourceGenerationId::new("1")),
            status: DocumentLifecycleStatus::Vectorized,
            updated_at: timestamp(),
            chunk_count: 1,
            vector_point_count: 1,
            error: None,
            cleanup_status: None,
        })
        .collect::<Vec<_>>();

    write_document_statuses(&ledger, &statuses, 2)
        .await
        .expect("write bounded status batches");

    let batches = ledger.document_status_update_batches().await;
    assert_eq!(batches.iter().map(Vec::len).collect::<Vec<_>>(), [2, 2, 1]);
    assert_eq!(
        batches.into_iter().flatten().collect::<Vec<_>>(),
        statuses
            .iter()
            .map(|status| status.document_id.clone())
            .collect::<Vec<_>>(),
        "every status must be written exactly once in input order"
    );
}

#[test]
fn only_the_last_vector_pool_of_the_final_source_batch_is_final() {
    assert!(!is_final_vector_batch(false, 1, 2));
    assert!(!is_final_vector_batch(true, 0, 2));
    assert!(is_final_vector_batch(true, 1, 2));
}

#[test]
fn oversized_document_is_split_into_bounded_chunk_windows() {
    let max_chunks = 512;
    let chunk_count = max_chunks * 2 + 1;
    let batches = chunk_batches(vec![prepared_document(chunk_count)], max_chunks);

    assert_eq!(batches.len(), 3);
    assert!(batches.iter().all(|batch| {
        batch
            .iter()
            .map(|document| document.chunks.len())
            .sum::<usize>()
            <= max_chunks
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
    let max_chunks = 512;
    let chunk_count = max_chunks + 7;
    let mut merged = VectorizeResult::default();
    for window in split_oversized_document(prepared_document(chunk_count), max_chunks) {
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
fn redaction_failure_omits_only_the_forbidden_chunk() {
    let mut document = axon_vectors::testing::test_prepared_document();
    document.chunks[1].content = "API_KEY=abcdef0123456789abcdef0123".to_string(); // gitleaks:allow — synthetic redaction fixture
    let mut embeddings =
        axon_vectors::testing::test_embedding_result_for(&document, "text-embedding-test", 3);

    let batch = point_batch(
        axon_vectors::testing::test_collection_spec(3),
        std::slice::from_ref(&document),
        &mut embeddings,
    )
    .expect("the clean chunk remains eligible for vectorization");

    assert_eq!(batch.skipped_redaction, 1);
    assert_eq!(batch.batch.points.len(), 1);
    assert_eq!(batch.batch.points[0].chunk_id, document.chunks[0].chunk_id);
}

#[test]
fn successful_short_upsert_is_rejected() {
    let error = validate_upsert_counts(3, 3, 2).expect_err("short write must fail");
    assert!(error.to_string().contains("vector upsert short write"));
    validate_upsert_counts(3, 3, 3).expect("complete write");
}

fn vector_write(points: u64) -> VectorStoreWriteResult {
    VectorStoreWriteResult {
        header: StageResultHeader {
            job_id: JobId::new(uuid::Uuid::from_u128(1)),
            stage_id: StageId::new(uuid::Uuid::from_u128(2)),
            phase: PipelinePhase::Upserting,
            status: LifecycleStatus::Completed,
            started_at: timestamp(),
            completed_at: Some(timestamp()),
            counts: StageCounts {
                items_total: Some(points),
                items_done: points,
                documents_total: None,
                documents_done: 0,
                chunks_total: Some(points),
                chunks_done: points,
                bytes_total: None,
                bytes_done: 0,
            },
            warnings: Vec::new(),
            error: None,
        },
        collection: "progress-test".to_string(),
        points_attempted: points,
        points_written: points,
        payload_indexes_created: Vec::new(),
        usage: ProviderUsage {
            input_tokens: None,
            output_tokens: None,
            requests: 1,
            duration_ms: 0,
        },
    }
}

#[test]
fn vectorized_document_statuses_use_actual_point_counts() {
    let document = prepared_document(3);
    let document_id = document.document_id.clone();
    let points_by_document = [(document_id, 2)].into_iter().collect();
    let result = vectorize_result(
        vec![document],
        Vec::new(),
        &points_by_document,
        vector_write(2),
        0,
        &std::collections::BTreeMap::new(),
    );

    assert_eq!(result.points_written, 2);
    assert_eq!(result.document_statuses.len(), 1);
    assert_eq!(result.document_statuses[0].chunk_count, 3);
    assert_eq!(result.document_statuses[0].vector_point_count, 2);
    assert!(result.warnings.is_empty());
}

#[test]
fn vectorize_result_reports_redaction_skips_per_source_item() {
    let document = prepared_document(2);
    let document_id = document.document_id.clone();
    let source_item_key = document.source_item_key.clone();
    let points_by_document = [(document_id, 1)].into_iter().collect();
    let skips = [(source_item_key.clone(), 1)].into_iter().collect();

    let result = vectorize_result(
        vec![document],
        Vec::new(),
        &points_by_document,
        vector_write(1),
        1,
        &skips,
    );

    assert_eq!(result.points_written, 1);
    assert_eq!(result.document_statuses[0].vector_point_count, 1);
    let warning = result
        .warnings
        .iter()
        .find(|warning| warning.code == "source.vectorize.redaction_skipped_chunks")
        .expect("redaction skip warning");
    assert_eq!(warning.source_item_key.as_ref(), Some(&source_item_key));
    assert!(warning.message.contains("skipped 1 chunk"));
}

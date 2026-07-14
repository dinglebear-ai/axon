use std::sync::Arc;

use axon_adapters::boundary::FakeAdapterProviders;
use axon_api::source::*;
use axon_core::boundary::{ArtifactStore, DocumentCache, FakeCoreBoundaries};
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::store::FakeVectorStore;

use crate::source::prune::drain_cleanup_debt_full_with_boundaries;

use super::{WebSourceIndexInput, document_cache_boundary, index_web_source};

fn web_input(core: FakeCoreBoundaries, output: OutputPolicy) -> WebSourceIndexInput {
    web_input_with_text(
        core,
        output,
        "# Intro\n\nClean page body from the fake fetch provider.",
    )
}

fn web_input_with_text(
    core: FakeCoreBoundaries,
    output: OutputPolicy,
    text: impl Into<String>,
) -> WebSourceIndexInput {
    let providers = Arc::new(FakeAdapterProviders::new().with_fetch_text(text));
    let mut crawl_options = MetadataMap::new();
    crawl_options.insert("render_mode".to_string(), serde_json::json!("http"));
    crawl_options.insert(
        "warc_path".to_string(),
        serde_json::json!("artifact://web/source.warc"),
    );
    WebSourceIndexInput {
        source: "https://docs.example.test/intro".to_string(),
        scope: SourceScope::Page,
        map_urls: Vec::new(),
        crawl_options,
        output,
        collection: "axon-web-artifact-test".to_string(),
        owner_id: "test-owner".to_string(),
        job_id: JobId::new(uuid::Uuid::from_u128(0x404)),
        embedding_provider_id: ProviderId::new("fake-embedding"),
        vector_provider_id: ProviderId::new("fake-vector"),
        embedding_model: "fake-embedding".to_string(),
        embedding_dimensions: 8,
        auth_snapshot: None,
        embed: false,
        fetch_provider: providers.clone(),
        render_provider: providers,
        artifact_store: Arc::new(core),
        event_store: None,
    }
}

#[tokio::test]
async fn warc_output_is_artifact_store_backed() {
    let core = FakeCoreBoundaries::new();
    let mut output = OutputPolicy::default();
    output.artifact_mode = ArtifactMode::Always;

    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let result = index_web_source(
        web_input(core.clone(), output),
        &ledger,
        &embedder,
        &vectors,
    )
    .await
    .expect("source run");

    let warc = result
        .artifacts
        .iter()
        .find(|artifact| artifact.artifact_kind == ArtifactKind::Warc)
        .expect("warc artifact");
    assert!(
        warc.content_hash
            .as_ref()
            .expect("hash")
            .starts_with("sha256:")
    );

    let stored = ArtifactStore::get(
        &core,
        ArtifactHandle {
            artifact_id: warc.artifact_id.clone(),
            artifact_kind: ArtifactKind::Warc,
            uri: Some(warc.uri.clone()),
        },
    )
    .await
    .expect("stored warc artifact");
    assert_eq!(stored.metadata["producer"], "web_source");
    assert_eq!(stored.metadata["job_id"], result.job_id.0.to_string());
    assert_eq!(stored.metadata["source_id"], result.source_id.0);
}

#[tokio::test]
async fn scrape_clean_content_respects_output_policy() {
    let core = FakeCoreBoundaries::new();
    let output = OutputPolicy {
        response_mode: ResponseMode::Inline,
        inline_limit_bytes: 4096,
        artifact_mode: ArtifactMode::None,
        ..OutputPolicy::default()
    };
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let result = index_web_source(web_input(core, output), &ledger, &embedder, &vectors)
        .await
        .expect("source run");

    let content = result
        .inline
        .expect("inline result")
        .content
        .expect("inline content");
    let ContentRef::InlineText { text } = content else {
        panic!("expected inline text content");
    };
    assert!(text.contains("Intro"));
    assert!(result.artifacts.is_empty());
}

#[test]
fn source_result_serializes_artifacts_when_present() {
    let artifact = ArtifactRef {
        artifact_id: ArtifactId::new("art_clean"),
        artifact_kind: ArtifactKind::NormalizedContent,
        uri: "artifact://clean/art_clean".to_string(),
        size_bytes: Some(10),
        content_hash: Some("sha256:test".to_string()),
        created_at: Timestamp("2026-07-13T00:00:00Z".to_string()),
    };
    let result = crate::source::result_map::to_source_result(
        SourceKind::Web,
        AdapterRef {
            name: "web".to_string(),
            version: "test".to_string(),
        },
        SourceScope::Page,
        "https://docs.example.test/intro".to_string(),
        crate::source::result_map::IndexCounts {
            job_id: JobId::new(uuid::Uuid::from_u128(1)),
            source_id: SourceId::new("src_web"),
            generation: SourceGenerationId::new("gen_web"),
            documents_prepared: 1,
            chunks_prepared: 1,
            vector_points_written: 0,
            removed: 0,
            graph_candidates: Vec::new(),
            warnings: Vec::new(),
            artifacts: vec![artifact],
            inline: None,
        },
        GraphWriteSummary {
            nodes_upserted: 0,
            edges_upserted: 0,
            evidence_records: 0,
            degraded: false,
        },
    );

    let json = serde_json::to_value(&result).expect("serialize source result");
    assert_eq!(json["artifacts"][0]["artifact_id"], "art_clean");
}

#[tokio::test]
async fn cleanup_debt_deletes_artifacts_and_cache_entries() {
    let core = FakeCoreBoundaries::new();
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source_summary()).await.unwrap();

    let handle = ArtifactStore::put(
        &core,
        ArtifactWriteRequest {
            kind: ArtifactKind::NormalizedContent,
            content_type: "text/markdown".to_string(),
            content: ContentRef::InlineText {
                text: "old content".to_string(),
            },
            source_id: Some(SourceId::new("src_web_artifacts")),
            job_id: Some(JobId::new(uuid::Uuid::from_u128(3))),
            metadata: MetadataMap::new(),
        },
    )
    .await
    .unwrap();

    let cache_key = DocumentCacheKey {
        source_id: SourceId::new("src_web_artifacts"),
        source_item_key: SourceItemKey::new("https://docs.example.test/old"),
        generation: Some(SourceGenerationId::new("gen_old")),
    };
    DocumentCache::put(
        &core,
        cache_key.clone(),
        CachedDocument {
            document: SourceDocument {
                document_id: DocumentId::new("doc_old"),
                source_id: cache_key.source_id.clone(),
                source_item_key: cache_key.source_item_key.clone(),
                canonical_uri: "https://docs.example.test/old".to_string(),
                content_kind: ContentKind::Markdown,
                content: ContentRef::InlineText {
                    text: "old content".to_string(),
                },
                metadata: MetadataMap::new(),
                title: None,
                language: None,
                path: None,
                mime_type: None,
                structured_payload: None,
                artifact_id: Some(handle.artifact_id.clone()),
                chunk_hints: Vec::new(),
                parser_hints: Vec::new(),
            },
            cached_at: Timestamp("2026-07-13T00:00:00Z".to_string()),
        },
    )
    .await
    .unwrap();

    ledger
        .record_cleanup_debt(cleanup_debt(
            "artifact",
            CleanupDebtKind::ArtifactDelete,
            CleanupSelector::Artifact {
                artifact_id: handle.artifact_id.clone(),
            },
        ))
        .await
        .unwrap();
    ledger
        .record_cleanup_debt(cleanup_debt(
            "cache",
            CleanupDebtKind::CachePrune,
            CleanupSelector::CacheKeys {
                keys: vec![serde_json::to_string(&cache_key).unwrap()],
            },
        ))
        .await
        .unwrap();

    let vectors = FakeVectorStore::new("fake-vector");
    let counts = crate::source::result_map::IndexCounts {
        job_id: JobId::new(uuid::Uuid::from_u128(4)),
        source_id: SourceId::new("src_web_artifacts"),
        generation: SourceGenerationId::new("gen_current"),
        documents_prepared: 0,
        chunks_prepared: 0,
        vector_points_written: 0,
        removed: 0,
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        artifacts: Vec::new(),
        inline: None,
    };

    let summary = drain_cleanup_debt_full_with_boundaries(
        &ledger,
        &vectors,
        None,
        None,
        None,
        Some(&core),
        Some(&core),
        "axon-web-artifact-test",
        &counts,
    )
    .await;

    assert_eq!(summary.resolved, 2);
    assert_eq!(summary.failed, 0);
    assert!(
        ArtifactStore::get(&core, handle).await.is_err(),
        "artifact should be deleted"
    );
    assert!(
        DocumentCache::get(&core, cache_key)
            .await
            .unwrap()
            .is_none(),
        "cache key should be invalidated"
    );
}

#[tokio::test]
async fn second_web_generation_records_artifact_and_cache_cleanup_debt() {
    let core = FakeCoreBoundaries::new();
    let mut output = OutputPolicy::default();
    output.artifact_mode = ArtifactMode::Always;

    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");
    let cache = document_cache_boundary();
    let source_url = format!(
        "https://docs.example.test/intro-cache-{}",
        uuid::Uuid::new_v4()
    );

    let mut first_input =
        web_input_with_text(core.clone(), output.clone(), "# Intro\n\nfirst body");
    first_input.source = source_url.clone();
    first_input.embed = true;
    let first = index_web_source(first_input, &ledger, &embedder, &vectors)
        .await
        .expect("first source run");
    let first_manifest = ledger
        .get_manifest(first.source_id.clone(), first.generation.clone())
        .await
        .unwrap()
        .expect("first manifest");
    let first_item = first_manifest.items.first().expect("first item");
    let cache_key = DocumentCacheKey {
        source_id: first.source_id.clone(),
        source_item_key: first_item.source_item_key.clone(),
        generation: Some(first.generation.clone()),
    };
    let first_artifact_handles = first
        .artifacts
        .iter()
        .map(|artifact| ArtifactHandle {
            artifact_id: artifact.artifact_id.clone(),
            artifact_kind: artifact.artifact_kind,
            uri: Some(artifact.uri.clone()),
        })
        .collect::<Vec<_>>();

    let mut second_input = web_input_with_text(core.clone(), output, "# Intro\n\nsecond body");
    second_input.source = source_url;
    second_input.embed = true;
    let _second = index_web_source(second_input, &ledger, &embedder, &vectors)
        .await
        .expect("second source run");

    let pending = ledger
        .list_pending_cleanup_debt(first.source_id.clone())
        .await
        .expect("pending cleanup debt");
    assert!(
        pending
            .iter()
            .any(|debt| debt.kind == CleanupDebtKind::ArtifactDelete
                && matches!(
                    &debt.selector,
                    CleanupSelector::Artifact { artifact_id }
                        if artifact_id == &first_artifact_handles[0].artifact_id
                )),
        "second generation should create artifact cleanup debt for first generation output"
    );
    assert!(
        pending.iter().any(|debt| {
            debt.kind == CleanupDebtKind::CachePrune
                && matches!(
                    &debt.selector,
                    CleanupSelector::CacheKeys { keys }
                        if keys.contains(&serde_json::to_string(&cache_key).unwrap())
                )
        }),
        "second generation should create cache cleanup debt for the first generation document"
    );

    let counts = crate::source::result_map::IndexCounts {
        job_id: JobId::new(uuid::Uuid::from_u128(5)),
        source_id: first.source_id.clone(),
        generation: SourceGenerationId::new("gen_current"),
        documents_prepared: 0,
        chunks_prepared: 0,
        vector_points_written: 0,
        removed: 0,
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        artifacts: Vec::new(),
        inline: None,
    };
    let summary = drain_cleanup_debt_full_with_boundaries(
        &ledger,
        &vectors,
        None,
        None,
        None,
        Some(&core),
        Some(cache.as_ref()),
        "axon-web-artifact-test",
        &counts,
    )
    .await;

    assert_eq!(summary.failed, 0);
    assert!(
        summary.resolved >= 3,
        "artifact, cache, and vector cleanup debt should resolve"
    );
    for handle in first_artifact_handles {
        assert!(
            ArtifactStore::get(&core, handle).await.is_err(),
            "first generation artifact should be deleted"
        );
    }
    assert!(
        DocumentCache::get(cache.as_ref(), cache_key)
            .await
            .unwrap()
            .is_none(),
        "first generation cache entry should be invalidated"
    );
}

fn source_summary() -> SourceSummary {
    SourceSummary {
        source_id: SourceId::new("src_web_artifacts"),
        canonical_uri: "https://docs.example.test/".to_string(),
        display_name: "docs".to_string(),
        source_kind: SourceKind::Web,
        adapter: AdapterRef {
            name: "web".to_string(),
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
        created_at: Timestamp("2026-07-13T00:00:00Z".to_string()),
        updated_at: Timestamp("2026-07-13T00:00:00Z".to_string()),
        graph_node_ids: Vec::new(),
        last_refreshed_at: None,
        user_label: None,
        tags: Vec::new(),
        watch_id: None,
        last_job_id: None,
    }
}

fn cleanup_debt(id: &str, kind: CleanupDebtKind, selector: CleanupSelector) -> CleanupDebt {
    CleanupDebt {
        debt_id: CleanupDebtId::new(format!("debt_{id}")),
        job_id: JobId::new(uuid::Uuid::from_u128(0)),
        source_id: SourceId::new("src_web_artifacts"),
        generation: None,
        kind,
        selector,
        status: LifecycleStatus::Pending,
        created_at: Timestamp("2026-07-13T00:00:00Z".to_string()),
        attempts: 0,
        last_error: None,
        next_retry_at: None,
        completed_at: None,
    }
}

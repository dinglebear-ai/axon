use axon_api::source::*;

use super::*;

#[tokio::test]
async fn fake_core_boundaries_cover_artifact_config_cache_rate_and_health() {
    let fake = FakeCoreBoundaries::new();
    let handle = ArtifactStore::put(
        &fake,
        ArtifactWriteRequest {
            kind: ArtifactKind::Report,
            content_type: "text/plain".to_string(),
            content: ContentRef::InlineText {
                text: "hello".to_string(),
            },
            source_id: None,
            job_id: None,
            metadata: MetadataMap::new(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        ArtifactStore::get(&fake, handle)
            .await
            .unwrap()
            .content_type,
        "text/plain"
    );
    assert!(fake.validate().await.unwrap().valid);
    assert_eq!(
        fake.snapshot().await.unwrap(),
        ConfigSnapshotId::new("cfg_fake")
    );

    let cache_key = DocumentCacheKey {
        source_id: SourceId::new("src"),
        source_item_key: SourceItemKey::new("item"),
        generation: None,
    };
    DocumentCache::put(
        &fake,
        cache_key.clone(),
        CachedDocument {
            document: SourceDocument {
                document_id: DocumentId::new("doc"),
                source_id: SourceId::new("src"),
                source_item_key: SourceItemKey::new("item"),
                canonical_uri: "fake://doc".to_string(),
                content_kind: ContentKind::PlainText,
                content: ContentRef::InlineText {
                    text: "cached".to_string(),
                },
                metadata: MetadataMap::new(),
                title: None,
                language: None,
                path: None,
                mime_type: None,
                structured_payload: None,
                artifact_id: None,
                chunk_hints: Vec::new(),
                parser_hints: Vec::new(),
            },
            cached_at: Timestamp("2026-07-01T00:00:00Z".to_string()),
        },
    )
    .await
    .unwrap();
    assert!(
        DocumentCache::get(&fake, cache_key)
            .await
            .unwrap()
            .is_some()
    );
    DocumentCache::reset(&fake).await.unwrap();
    assert!(
        DocumentCache::get(
            &fake,
            DocumentCacheKey {
                source_id: SourceId::new("src"),
                source_item_key: SourceItemKey::new("item"),
                generation: None,
            },
        )
        .await
        .unwrap()
        .is_none()
    );

    assert_eq!(
        fake.acquire(RateLimitRequest {
            provider_id: ProviderId::new("fake"),
            units: 1,
            priority: JobPriority::Interactive,
        })
        .await
        .unwrap()
        .units,
        1
    );
    assert_eq!(
        fake.probe(HealthProbeRequest {
            provider_id: ProviderId::new("fake"),
            provider_kind: ProviderKind::HealthProbe,
        })
        .await
        .unwrap()
        .status,
        HealthStatus::Healthy
    );
}

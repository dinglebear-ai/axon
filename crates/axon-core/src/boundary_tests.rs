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
        ArtifactStore::get(&fake, handle.clone())
            .await
            .unwrap()
            .content_type,
        "text/plain"
    );
    let second_handle = ArtifactStore::put(
        &fake,
        ArtifactWriteRequest {
            kind: ArtifactKind::Report,
            content_type: "text/plain".to_string(),
            content: ContentRef::InlineText {
                text: "world".to_string(),
            },
            source_id: None,
            job_id: None,
            metadata: MetadataMap::new(),
        },
    )
    .await
    .unwrap();
    assert_ne!(handle.artifact_id, second_handle.artifact_id);
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

#[tokio::test]
async fn file_artifact_store_ids_are_owner_unique_for_identical_content() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = FileArtifactStore::new(temp.path());
    let first = ArtifactStore::put(
        &store,
        ArtifactWriteRequest {
            kind: ArtifactKind::NormalizedContent,
            content_type: "text/markdown".to_string(),
            content: ContentRef::InlineText {
                text: "same bytes".to_string(),
            },
            source_id: Some(SourceId::new("src_one")),
            job_id: Some(JobId::new(uuid::Uuid::from_u128(1))),
            metadata: MetadataMap::new(),
        },
    )
    .await
    .unwrap();
    let second = ArtifactStore::put(
        &store,
        ArtifactWriteRequest {
            kind: ArtifactKind::NormalizedContent,
            content_type: "text/markdown".to_string(),
            content: ContentRef::InlineText {
                text: "same bytes".to_string(),
            },
            source_id: Some(SourceId::new("src_two")),
            job_id: Some(JobId::new(uuid::Uuid::from_u128(2))),
            metadata: MetadataMap::new(),
        },
    )
    .await
    .unwrap();

    assert_ne!(first.artifact_id, second.artifact_id);
    ArtifactStore::delete(&store, first).await.unwrap();
    assert!(
        ArtifactStore::get(&store, second).await.is_ok(),
        "deleting one owner artifact must not remove identical bytes from another owner"
    );
}

#[tokio::test]
async fn fake_core_boundaries_report_health_override() {
    let fake = FakeCoreBoundaries::new().with_health(HealthStatus::Cooling);

    let report = fake
        .probe(HealthProbeRequest {
            provider_id: ProviderId::new("fake"),
            provider_kind: ProviderKind::HealthProbe,
        })
        .await
        .unwrap();
    let capability = HealthProbe::capabilities(&fake).await.unwrap();

    assert_eq!(report.status, HealthStatus::Cooling);
    assert_eq!(capability.health, HealthStatus::Cooling);
    assert_eq!(
        ArtifactStore::capabilities(&fake).await.unwrap().0.health,
        HealthStatus::Cooling
    );
    assert_eq!(
        ConfigStore::capabilities(&fake).await.unwrap().0.health,
        HealthStatus::Cooling
    );
    assert_eq!(
        DocumentCache::capabilities(&fake).await.unwrap().0.health,
        HealthStatus::Cooling
    );
}

#[tokio::test]
async fn fake_core_provider_capabilities_reflect_failure_mode() {
    let timeout = FakeCoreBoundaries::new().with_mode(FakeCoreMode::Timeout);
    assert_eq!(
        RateLimiter::capabilities(&timeout).await.unwrap().health,
        HealthStatus::Degraded
    );

    let rate_limited = FakeCoreBoundaries::new().with_mode(FakeCoreMode::RateLimited);
    let capability = RateLimiter::capabilities(&rate_limited).await.unwrap();
    assert_eq!(capability.health, HealthStatus::Cooling);
    assert!(capability.cooldown_until.is_some());
    assert_eq!(
        capability.last_error.unwrap().code.to_string(),
        "provider.rate_limited"
    );

    let fake = FakeCoreBoundaries::new().with_mode(FakeCoreMode::Fatal);

    let capability = RateLimiter::capabilities(&fake).await.unwrap();

    assert_eq!(capability.health, HealthStatus::Unavailable);
    let error = capability.last_error.unwrap();
    assert_eq!(error.code.to_string(), "provider.fatal");
    assert_eq!(error.provider_id, Some("fake_ratelimiter".to_string()));
    assert!(!error.retryable);
}

#[tokio::test]
async fn fake_core_rate_limiter_returns_failure_modes_and_records_calls() {
    let fake = FakeCoreBoundaries::new().with_mode(FakeCoreMode::RateLimited);

    let err = fake
        .acquire(RateLimitRequest {
            provider_id: ProviderId::new("tei"),
            units: 1,
            priority: JobPriority::Background,
        })
        .await
        .unwrap_err();

    assert_eq!(err.code.to_string(), "provider.rate_limited");
    assert!(err.retryable);
    assert_eq!(fake.calls().await, vec!["rate_limit.acquire"]);
}

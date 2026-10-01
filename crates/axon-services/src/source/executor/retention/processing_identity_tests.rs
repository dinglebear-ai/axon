use super::*;
use axon_document::DocumentPreparer;

fn snapshot() -> JobConfigSnapshot<'static> {
    JobConfigSnapshot {
        source_kind: "local",
        source_ref: "file:///tmp/identity",
        collection: "docs",
        embedding_provider_id: "embed",
        vector_provider_id: "vectors",
        embedding_model: "model",
        embedding_dimensions: 64,
        embed: true,
        max_items: None,
    }
}
fn plan() -> SourcePlan {
    let request = SourceRequest::local_path("/tmp/identity", true);
    SourcePlan {
        job_id: JobId::new(uuid::Uuid::nil()),
        route: crate::source::routing::resolve_source_route(&request)
            .unwrap()
            .route,
        request,
        limits: EffectiveLimits {
            request: Default::default(),
            adapter_defaults: Default::default(),
            config_defaults: Default::default(),
            effective: Default::default(),
        },
        stage_plan: vec![],
        config_snapshot_id: ConfigSnapshotId::new("job"),
        provider_reservations: vec![],
    }
}
fn hash(plan: &SourcePlan, config: DocumentPreparerConfig, resident: usize) -> ConfigSnapshotId {
    configured_identity(snapshot(), &DocumentPreparer::new(config), resident, plan)
}

#[test]
fn schema_and_each_preparation_knob_invalidate_identity() {
    let config = DocumentPreparerConfig::default();
    let baseline = identity(snapshot(), config, None, PREPARATION_SCHEMA_VERSION);
    // The redaction correction must reprepare unchanged files whose previous
    // publication omitted chunks under the schema-8 detector policy.
    assert_ne!(
        baseline,
        identity(snapshot(), config, None, "axon-document/schema-8")
    );
    for modified in [
        DocumentPreparerConfig {
            max_content_bytes: config.max_content_bytes - 1,
            ..config
        },
        DocumentPreparerConfig {
            markdown_max_chars: config.markdown_max_chars + 1,
            ..config
        },
        DocumentPreparerConfig {
            markdown_min_chars: config.markdown_min_chars + 1,
            ..config
        },
        DocumentPreparerConfig {
            markdown_overlap_chars: config.markdown_overlap_chars + 1,
            ..config
        },
        DocumentPreparerConfig {
            minimum_chunk_chars: config.minimum_chunk_chars + 1,
            ..config
        },
    ] {
        assert_ne!(
            baseline,
            identity(snapshot(), modified, None, PREPARATION_SCHEMA_VERSION)
        );
    }
}

#[test]
fn provider_model_and_collection_remain_semantic() {
    let config = DocumentPreparerConfig::default();
    let base = snapshot();
    let baseline = identity(base, config, None, PREPARATION_SCHEMA_VERSION);
    for modified in [
        JobConfigSnapshot {
            embedding_model: "new-model",
            ..base
        },
        JobConfigSnapshot {
            embedding_provider_id: "new-provider",
            ..base
        },
        JobConfigSnapshot {
            vector_provider_id: "new-vector",
            ..base
        },
        JobConfigSnapshot {
            collection: "new-collection",
            ..base
        },
        JobConfigSnapshot {
            embedding_dimensions: 128,
            ..base
        },
        JobConfigSnapshot {
            embed: false,
            ..base
        },
    ] {
        assert_ne!(
            baseline,
            identity(modified, config, None, PREPARATION_SCHEMA_VERSION)
        );
    }
}

#[test]
fn normalized_chunk_limits_share_identity() {
    let baseline = DocumentPreparerConfig {
        markdown_max_chars: 0,
        markdown_min_chars: 0,
        markdown_overlap_chars: 999,
        ..Default::default()
    };
    let equivalent = DocumentPreparerConfig {
        markdown_max_chars: 1,
        markdown_min_chars: 1,
        markdown_overlap_chars: 0,
        minimum_chunk_chars: 1,
        ..baseline
    };
    let left = DocumentPreparer::new(baseline).semantic_config();
    assert_eq!(left, equivalent);
    assert_eq!(
        hash(&plan(), baseline, usize::MAX),
        hash(&plan(), equivalent, usize::MAX)
    );
}

#[test]
fn effective_ceiling_changes_identity_but_nonbinding_budget_does_not() {
    let mut plan = plan();
    let config = DocumentPreparerConfig {
        max_content_bytes: 1000,
        ..Default::default()
    };
    let baseline = hash(&plan, config, 10_000);
    assert_eq!(baseline, hash(&plan, config, 20_000));
    assert_ne!(baseline, hash(&plan, config, 4000));
    plan.limits.effective.max_bytes_per_item = Some(500);
    assert_ne!(baseline, hash(&plan, config, 10_000));
    assert_eq!(hash(&plan, config, 10_000), hash(&plan, config, 2500));
}

#[test]
fn scan_and_admission_options_do_not_change_output_identity() {
    let mut plan = plan();
    let preparer = DocumentPreparer::default();
    let baseline = configured_identity(snapshot(), &preparer, usize::MAX, &plan);
    plan.limits.effective.max_items = Some(1);
    plan.limits.effective.max_total_bytes = Some(1);
    plan.route.source.metadata.insert(
        axon_adapters::acquisition::ACQUISITION_BATCH_BYTES_KEY.into(),
        1.into(),
    );
    plan.route
        .validated_options
        .values
        .insert("unrelated_secret".into(), "never-hash-this".into());
    let changed = JobConfigSnapshot {
        max_items: Some(1),
        ..snapshot()
    };
    assert_eq!(
        baseline,
        configured_identity(changed, &preparer, usize::MAX, &plan)
    );
}

#[test]
fn local_file_limit_uses_adapter_default_and_finite_effective_bound() {
    let mut plan = plan();
    let config = DocumentPreparerConfig::default();
    let baseline = hash(&plan, config, usize::MAX);
    assert_eq!(
        local_file_limit(&plan),
        Some(axon_adapters::DEFAULT_LOCAL_MAX_FILE_BYTES)
    );
    plan.route
        .validated_options
        .values
        .insert("max_file_bytes".into(), 1000.into());
    assert_ne!(baseline, hash(&plan, config, usize::MAX));
    plan.limits.effective.max_bytes_per_item = Some(500);
    let bounded = hash(&plan, config, usize::MAX);
    plan.route
        .validated_options
        .values
        .insert("max_file_bytes".into(), 2000.into());
    assert_eq!(bounded, hash(&plan, config, usize::MAX));
    plan.limits.effective.max_bytes_per_item = None;
    plan.route
        .validated_options
        .values
        .insert("max_file_bytes".into(), u64::MAX.into());
    assert_eq!(
        local_file_limit(&plan),
        Some(axon_adapters::acquisition::MAX_FILE_CONTENT_BYTES)
    );
    plan.route.source.source_kind = SourceKind::Git;
    assert_eq!(local_file_limit(&plan), None);
}

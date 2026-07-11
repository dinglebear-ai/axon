use std::fs;
use std::path::PathBuf;

use axon_api::source::*;
use uuid::Uuid;

pub(super) fn registry_options(dump_path: &std::path::Path) -> MetadataMap {
    let mut values = MetadataMap::new();
    values.insert(
        "registry_dump_path".to_string(),
        dump_path.to_string_lossy().to_string().into(),
    );
    values
}

pub(super) fn registry_options_all_versions(dump_path: &std::path::Path) -> MetadataMap {
    let mut values = registry_options(dump_path);
    values.insert("include_all_versions".to_string(), true.into());
    values
}

pub(super) fn write_dump(json: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("axon-registry-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("failed to create registry adapter test directory");
    let path = dir.join("dump.json");
    fs::write(&path, json).expect("failed to write registry dump fixture");
    path
}

pub(super) fn valid_dump_json() -> &'static str {
    r##"{
        "registry": "npm",
        "package": "lodash",
        "description": "Lodash modular utilities.",
        "homepage": "https://lodash.com",
        "license": "MIT",
        "author": "jdd",
        "keywords": ["array", "util"],
        "versions": [
            {
                "version": "4.17.20",
                "readme": "# lodash 4.17.20\n\nOlder release.",
                "is_latest": false
            },
            {
                "version": "4.17.21",
                "readme": "# lodash\n\nA modern JavaScript utility library.",
                "description": "Lodash modular utilities.",
                "published_at": "2021-02-20T00:00:00Z",
                "is_latest": true
            }
        ]
    }"##
}

pub(super) fn huggingface_dump_json() -> &'static str {
    r##"{
        "registry": "huggingface",
        "package": "bert-base-uncased",
        "description": "BERT base model (uncased).",
        "homepage": "https://huggingface.co/bert-base-uncased",
        "license": "apache-2.0",
        "author": "google",
        "keywords": ["nlp", "transformers"],
        "versions": [
            {
                "version": "main",
                "readme": "# bert-base-uncased\n\nPretrained BERT model card.",
                "description": "BERT base model (uncased).",
                "is_latest": true
            }
        ]
    }"##
}

pub(super) fn source_plan(dump_path: PathBuf, scope: SourceScope) -> SourcePlan {
    source_plan_for("pkg://npm/lodash", dump_path, scope)
}

pub(super) fn source_plan_for(
    canonical_uri: &str,
    dump_path: PathBuf,
    scope: SourceScope,
) -> SourcePlan {
    let canonical_uri = canonical_uri.to_string();
    SourcePlan {
        job_id: JobId::new(Uuid::from_u128(298_009)),
        request: SourceRequest::new(canonical_uri.clone()),
        route: RoutePlan {
            source: ResolvedSource {
                source: canonical_uri.clone(),
                canonical_uri: canonical_uri.clone(),
                source_id: SourceId::from("src_registry_test"),
                source_kind: SourceKind::Registry,
                adapter: AdapterRef {
                    name: "registry".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
                default_scope: scope,
                available_scopes: vec![scope],
                authority: AuthorityLevel::Inferred,
                confidence: 1.0,
                reason: "test".to_string(),
                graph: Vec::new(),
                warnings: Vec::new(),
                metadata: MetadataMap::new(),
            },
            adapter: AdapterRef {
                name: "registry".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            scope,
            provider_requirements: Vec::new(),
            credential_requirements: Vec::new(),
            execution_affinity: ExecutionAffinity::Worker,
            safety_class: SafetyClass::LocalFilesystem,
            option_schema_id: "adapter:registry:options:v1".to_string(),
            validated_options: AdapterOptions {
                values: registry_options(&dump_path),
            },
            chunking_hints: Vec::new(),
            parser_hints: Vec::new(),
            graph_fact_kinds: Vec::new(),
            watch_supported: true,
            refresh_supported: true,
        },
        stage_plan: Vec::new(),
        limits: EffectiveLimits {
            request: SourceLimits::default(),
            adapter_defaults: SourceLimits::default(),
            config_defaults: SourceLimits::default(),
            effective: SourceLimits::default(),
        },
        config_snapshot_id: ConfigSnapshotId::from("cfg_registry_test"),
        provider_reservations: Vec::new(),
    }
}

pub(super) fn manifest_diff(plan: &SourcePlan, items: Vec<ManifestItem>) -> SourceManifestDiff {
    let added_count = items.len() as u64;
    SourceManifestDiff {
        header: StageResultHeader {
            job_id: plan.job_id,
            stage_id: StageId::new(Uuid::from_u128(298_010)),
            phase: PipelinePhase::Diffing,
            status: LifecycleStatus::Completed,
            started_at: timestamp(),
            completed_at: Some(timestamp()),
            counts: StageCounts {
                items_total: Some(items.len() as u64),
                items_done: items.len() as u64,
                documents_total: None,
                documents_done: 0,
                chunks_total: None,
                chunks_done: 0,
                bytes_total: None,
                bytes_done: 0,
            },
            warnings: Vec::new(),
            error: None,
        },
        source_id: plan.route.source.source_id.clone(),
        previous_generation: None,
        next_generation: SourceGenerationId::from("gen_registry_test"),
        added: items,
        modified: Vec::new(),
        removed: Vec::new(),
        unchanged: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
        counts: DiffCounts {
            added: added_count,
            modified: 0,
            removed: 0,
            unchanged: 0,
            skipped: 0,
            failed: 0,
        },
    }
}

pub(super) fn timestamp() -> Timestamp {
    Timestamp("2026-07-01T00:00:00Z".to_string())
}

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use axon_api::source::*;
use uuid::Uuid;

pub(super) fn local_options() -> MetadataMap {
    let mut values = MetadataMap::new();
    values.insert("include_globs".to_string(), vec!["**/*.rs"].into());
    values.insert("exclude_globs".to_string(), vec!["target/**"].into());
    values.insert("respect_gitignore".to_string(), true.into());
    values.insert("follow_symlinks".to_string(), false.into());
    values.insert("max_file_bytes".to_string(), 1024.into());
    values.insert("binary_policy".to_string(), "skip".into());
    values.insert("watch_policy".to_string(), "manual".into());
    values
}

pub(super) fn binary_options(policy: &str) -> MetadataMap {
    let mut values = MetadataMap::new();
    values.insert("binary_policy".to_string(), policy.into());
    values
}

pub(super) fn source_plan(path: PathBuf, scope: SourceScope) -> SourcePlan {
    let canonical_uri = format!("local://{}", slug(&path));
    SourcePlan {
        job_id: JobId::new(Uuid::from_u128(298)),
        request: SourceRequest::new(path.to_string_lossy().to_string()),
        route: RoutePlan {
            source: ResolvedSource {
                source: path.to_string_lossy().to_string(),
                canonical_uri: canonical_uri.clone(),
                source_id: SourceId::from("src_local_test"),
                source_kind: SourceKind::Local,
                adapter: AdapterRef {
                    name: "local".to_string(),
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
                name: "local".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            scope,
            provider_requirements: Vec::new(),
            credential_requirements: Vec::new(),
            execution_affinity: ExecutionAffinity::Worker,
            safety_class: SafetyClass::LocalFilesystem,
            option_schema_id: "adapter:local:options:v1".to_string(),
            validated_options: AdapterOptions::default(),
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
        config_snapshot_id: ConfigSnapshotId::from("cfg_local_test"),
        provider_reservations: Vec::new(),
    }
}

pub(super) fn manifest_diff(plan: &SourcePlan, items: Vec<ManifestItem>) -> SourceManifestDiff {
    let added_count = items.len() as u64;
    SourceManifestDiff {
        header: StageResultHeader {
            job_id: plan.job_id,
            stage_id: StageId::new(Uuid::from_u128(29801)),
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
        next_generation: SourceGenerationId::from("gen_local_test"),
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

pub(super) fn temp_source_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("axon-local-test-{}", Uuid::new_v4()));
    if let Err(err) = fs::create_dir_all(&dir) {
        panic!("failed to create local adapter test directory: {err}");
    }
    dir
}

fn slug(path: &Path) -> String {
    let mut counts = BTreeMap::new();
    path.components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|part| !part.is_empty() && *part != "/")
        .map(|part| {
            let count = counts.entry(part.to_string()).or_insert(0);
            *count += 1;
            if *count == 1 {
                part.to_string()
            } else {
                format!("{part}-{count}")
            }
        })
        .collect::<Vec<_>>()
        .join("-")
}

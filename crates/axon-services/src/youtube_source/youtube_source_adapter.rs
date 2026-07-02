use anyhow::Context;
use axon_adapters::SourceAdapter;
use axon_adapters::youtube::{YoutubeSourceAdapter, parse_youtube_target};
use axon_api::source::*;
use sha2::{Digest, Sha256};

use super::{YOUTUBE_ADAPTER_VERSION, YoutubeSourceIndexInput};

#[derive(Debug, Clone)]
pub(super) struct YoutubeAdapterRun {
    pub source_id: SourceId,
    pub canonical_uri: String,
    pub adapter: AdapterRef,
    pub scope: SourceScope,
    pub plan: SourcePlan,
}

pub(super) fn resolve_adapter_run(
    input: &YoutubeSourceIndexInput,
) -> anyhow::Result<YoutubeAdapterRun> {
    let target = parse_youtube_target(&input.target)
        .map_err(|err| anyhow::anyhow!("invalid youtube target: {}", err.message))
        .with_context(|| format!("failed to parse youtube target {}", input.target))?;
    let source_id = youtube_source_id(&target.canonical_uri);
    let adapter = youtube_adapter_ref();
    let scope = target.scope;
    let plan = source_plan(
        input,
        &target.canonical_uri,
        &source_id,
        adapter.clone(),
        scope,
    );
    Ok(YoutubeAdapterRun {
        source_id,
        canonical_uri: target.canonical_uri,
        adapter,
        scope,
        plan,
    })
}

pub(super) async fn discover_manifest(run: &YoutubeAdapterRun) -> anyhow::Result<SourceManifest> {
    Ok(YoutubeSourceAdapter::new().discover(&run.plan).await?)
}

pub(super) async fn normalize_changed_documents(
    run: &YoutubeAdapterRun,
    diff: &SourceManifestDiff,
) -> anyhow::Result<Vec<SourceDocument>> {
    let acquisition = YoutubeSourceAdapter::new().acquire(&run.plan, diff).await?;
    Ok(YoutubeSourceAdapter::new()
        .normalize(&run.plan, acquisition)
        .await?
        .data)
}

pub(super) fn collection_spec(collection: &str, dimensions: u32) -> CollectionSpec {
    CollectionSpec {
        collection: collection.to_string(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions,
            distance: VectorDistance::Cosine,
        },
        payload_indexes: vec![
            payload_index("source_id"),
            payload_index("source_generation"),
            payload_index("source_item_key"),
            payload_index("document_id"),
            payload_index("chunk_id"),
        ],
        sparse: None,
        aliases: Vec::new(),
        distance: Some(VectorDistance::Cosine),
        metadata: MetadataMap::new(),
    }
}

pub(super) fn source_summary(
    input: &YoutubeSourceIndexInput,
    run: &YoutubeAdapterRun,
) -> SourceSummary {
    SourceSummary {
        source_id: run.source_id.clone(),
        canonical_uri: run.canonical_uri.clone(),
        display_name: run.canonical_uri.clone(),
        source_kind: SourceKind::Youtube,
        adapter: run.adapter.clone(),
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
        tags: vec![format!("{:?}", run.scope).to_ascii_lowercase()],
        watch_id: None,
        last_job_id: Some(input.job_id),
    }
}

pub(super) fn youtube_adapter_ref() -> AdapterRef {
    AdapterRef {
        name: "youtube".to_string(),
        version: YOUTUBE_ADAPTER_VERSION.to_string(),
    }
}

pub(crate) fn youtube_source_id(canonical_uri: &str) -> SourceId {
    SourceId::new(format!("src_youtube_{}", stable_token(canonical_uri)))
}

pub(crate) fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}

fn stable_token(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    let mut token = String::with_capacity(24);
    for byte in &digest[..12] {
        use std::fmt::Write as _;
        let _ = write!(&mut token, "{byte:02x}");
    }
    token
}

fn source_plan(
    input: &YoutubeSourceIndexInput,
    canonical_uri: &str,
    source_id: &SourceId,
    adapter: AdapterRef,
    scope: SourceScope,
) -> SourcePlan {
    let mut values = MetadataMap::new();
    values.insert(
        "youtube_dump_path".to_string(),
        serde_json::json!(input.youtube_dump_path.to_string_lossy()),
    );
    SourcePlan {
        job_id: input.job_id,
        request: SourceRequest::new(input.target.clone()),
        route: RoutePlan {
            source: ResolvedSource {
                requested_uri: input.target.clone(),
                canonical_uri: canonical_uri.to_string(),
                source_id: source_id.clone(),
                source_kind: SourceKind::Youtube,
                display_name: canonical_uri.to_string(),
                candidate_adapters: vec![AdapterCandidate {
                    adapter: adapter.clone(),
                    supported_scopes: vec![scope],
                    confidence: 1.0,
                    reason: "target youtube source".to_string(),
                }],
                default_scope: scope,
                available_scopes: vec![scope],
                authority: AuthorityLevel::UserPinned,
                confidence: 1.0,
                reason: "target youtube source".to_string(),
                authority_hint: None,
                warnings: Vec::new(),
            },
            adapter,
            scope,
            provider_requirements: Vec::new(),
            credential_requirements: Vec::new(),
            execution_affinity: ExecutionAffinity::Worker,
            safety_class: SafetyClass::LocalFilesystem,
            option_schema_id: "adapter:youtube:options:v1".to_string(),
            validated_options: AdapterOptions { values },
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
        config_snapshot_id: ConfigSnapshotId::new("cfg_youtube_source"),
        provider_reservations: provider_reservations(input),
    }
}

fn provider_reservations(input: &YoutubeSourceIndexInput) -> Vec<ProviderReservationRequest> {
    let mut reservations = Vec::new();
    if input.embedding_reservations.is_some() {
        reservations.push(ProviderReservationRequest {
            provider_kind: ProviderKind::Embedding,
            priority: JobPriority::Background,
            units: 1,
            reason: "youtube source embedding".to_string(),
        });
    }
    if input.vector_reservations.is_some() {
        reservations.push(ProviderReservationRequest {
            provider_kind: ProviderKind::Vector,
            priority: JobPriority::Background,
            units: 1,
            reason: "youtube source vector write".to_string(),
        });
    }
    reservations
}

fn payload_index(field_name: &str) -> PayloadIndexSpec {
    PayloadIndexSpec {
        field_name: field_name.to_string(),
        field_schema: PayloadFieldSchema::Keyword,
        required_for_filters: true,
    }
}

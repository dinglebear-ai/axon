use anyhow::Context;
use axon_adapters::SourceAdapter;
use axon_adapters::reddit::{RedditSourceAdapter, RedditTarget, parse_reddit_target};
use axon_api::source::*;
use axon_route::source_id::source_id as route_source_id;

use super::{REDDIT_ADAPTER_VERSION, RedditSourceIndexInput};

#[derive(Debug, Clone)]
pub(super) struct RedditAdapterRun {
    pub source_id: SourceId,
    pub adapter: AdapterRef,
    pub scope: SourceScope,
    pub plan: SourcePlan,
}

pub(super) fn resolve_adapter_run(
    input: &RedditSourceIndexInput,
) -> anyhow::Result<RedditAdapterRun> {
    let target = parse_reddit_target(&input.target)
        .map_err(|err| anyhow::anyhow!(err))
        .with_context(|| "invalid reddit target")?;
    let canonical_uri = canonical_uri_for(&target);
    let scope = scope_for(&target);
    let source_id = route_source_id(SourceKind::Reddit, &canonical_uri);
    let adapter = reddit_adapter_ref();
    let plan = source_plan(
        input,
        &target,
        &canonical_uri,
        &source_id,
        adapter.clone(),
        scope,
    );
    Ok(RedditAdapterRun {
        source_id,
        adapter,
        scope,
        plan,
    })
}

/// Compute a reddit source's `SourceId` from its target string alone, without
/// building a full `SourcePlan`. Used by the job wrapper to stamp the job's
/// `source_id` before the index pipeline runs, mirroring the local adapter's
/// `local_source_id(&root)` pre-job-creation call.
pub(crate) fn reddit_source_id(target: &str) -> anyhow::Result<SourceId> {
    let target = parse_reddit_target(target)
        .map_err(|err| anyhow::anyhow!(err))
        .with_context(|| "invalid reddit target")?;
    Ok(route_source_id(
        SourceKind::Reddit,
        &canonical_uri_for(&target),
    ))
}

pub(super) async fn discover_manifest(run: &RedditAdapterRun) -> anyhow::Result<SourceManifest> {
    Ok(RedditSourceAdapter::new().discover(&run.plan).await?)
}

pub(super) async fn normalize_changed_documents(
    run: &RedditAdapterRun,
    diff: &SourceManifestDiff,
) -> anyhow::Result<Vec<SourceDocument>> {
    let acquisition = RedditSourceAdapter::new().acquire(&run.plan, diff).await?;
    Ok(RedditSourceAdapter::new()
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
        sparse: Some(SparseVectorConfig {
            name: "bm42".to_string(),
            modifier: SparseVectorModifier::Idf,
        }),
        aliases: Vec::new(),
        distance: Some(VectorDistance::Cosine),
        metadata: MetadataMap::new(),
    }
}

pub(super) fn source_summary(
    input: &RedditSourceIndexInput,
    run: &RedditAdapterRun,
) -> SourceSummary {
    SourceSummary {
        source_id: run.source_id.clone(),
        canonical_uri: run.plan.route.source.canonical_uri.clone(),
        display_name: run.plan.route.source.display_name.clone(),
        source_kind: SourceKind::Reddit,
        adapter: run.adapter.clone(),
        authority: AuthorityLevel::Inferred,
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

fn reddit_adapter_ref() -> AdapterRef {
    AdapterRef {
        name: "reddit".to_string(),
        version: REDDIT_ADAPTER_VERSION.to_string(),
    }
}

pub(crate) fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}

fn canonical_uri_for(target: &RedditTarget) -> String {
    match target {
        RedditTarget::Subreddit(name) => format!("reddit://r/{name}"),
        RedditTarget::Thread(permalink) => format!("reddit://{}", permalink.trim_matches('/')),
    }
}

fn scope_for(target: &RedditTarget) -> SourceScope {
    match target {
        RedditTarget::Subreddit(_) => SourceScope::Subreddit,
        RedditTarget::Thread(_) => SourceScope::Thread,
    }
}

fn display_name_for(target: &RedditTarget) -> String {
    match target {
        RedditTarget::Subreddit(name) => format!("r/{name}"),
        RedditTarget::Thread(permalink) => permalink.clone(),
    }
}

fn source_plan(
    input: &RedditSourceIndexInput,
    target: &RedditTarget,
    canonical_uri: &str,
    source_id: &SourceId,
    adapter: AdapterRef,
    scope: SourceScope,
) -> SourcePlan {
    let requested_uri = input.target.clone();
    let source = SourceRequest::new(requested_uri.clone());
    SourcePlan {
        job_id: input.job_id,
        request: source,
        route: RoutePlan {
            source: ResolvedSource {
                requested_uri,
                canonical_uri: canonical_uri.to_string(),
                source_id: source_id.clone(),
                source_kind: SourceKind::Reddit,
                display_name: display_name_for(target),
                candidate_adapters: vec![AdapterCandidate {
                    adapter: adapter.clone(),
                    supported_scopes: vec![scope],
                    confidence: 1.0,
                    reason: "target reddit source".to_string(),
                }],
                default_scope: scope,
                available_scopes: vec![scope],
                authority: AuthorityLevel::Inferred,
                confidence: 1.0,
                reason: "target reddit source".to_string(),
                authority_hint: None,
                warnings: Vec::new(),
            },
            adapter,
            scope,
            provider_requirements: Vec::new(),
            credential_requirements: Vec::new(),
            execution_affinity: ExecutionAffinity::Worker,
            safety_class: SafetyClass::AuthenticatedNetwork,
            option_schema_id: "adapter:reddit:options:v1".to_string(),
            validated_options: AdapterOptions {
                values: adapter_options(input),
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
        config_snapshot_id: ConfigSnapshotId::new("cfg_reddit_source"),
        provider_reservations: provider_reservations(input),
    }
}

fn provider_reservations(input: &RedditSourceIndexInput) -> Vec<ProviderReservationRequest> {
    let mut reservations = Vec::new();
    if input.embedding_reservations.is_some() {
        reservations.push(ProviderReservationRequest {
            provider_kind: ProviderKind::Embedding,
            priority: JobPriority::Background,
            units: 1,
            reason: "reddit source embedding".to_string(),
        });
    }
    if input.vector_reservations.is_some() {
        reservations.push(ProviderReservationRequest {
            provider_kind: ProviderKind::Vector,
            priority: JobPriority::Background,
            units: 1,
            reason: "reddit source vector write".to_string(),
        });
    }
    reservations
}

fn adapter_options(input: &RedditSourceIndexInput) -> MetadataMap {
    let mut options = MetadataMap::new();
    options.insert(
        "reddit_dump_path".to_string(),
        serde_json::json!(input.dump_path.display().to_string()),
    );
    options
}

fn payload_index(field_name: &str) -> PayloadIndexSpec {
    PayloadIndexSpec {
        field_name: field_name.to_string(),
        field_schema: PayloadFieldSchema::Keyword,
        required_for_filters: true,
    }
}

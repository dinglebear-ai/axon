//! Git adapter dispatch — builds the `SourcePlan` for a prepared clone and
//! drives the `GitSourceAdapter` through discover / acquire / enrich / normalize.

use std::collections::BTreeMap;
use std::path::PathBuf;

use axon_adapters::SourceAdapter;
use axon_adapters::SourceEnricher;
use axon_adapters::git::{GitSourceAdapter, GitTarget, parse_git_target};
use axon_api::source::*;
use sha2::{Digest, Sha256};

use super::{GIT_ADAPTER_VERSION, GitSourceIndexInput};

#[derive(Debug, Clone)]
pub(super) struct GitAdapterRun {
    #[allow(dead_code)]
    pub repo_root: PathBuf,
    pub target: GitTarget,
    pub source_id: SourceId,
    pub adapter: AdapterRef,
    #[allow(dead_code)]
    pub scope: SourceScope,
    pub plan: SourcePlan,
}

pub(super) fn resolve_adapter_run(input: &GitSourceIndexInput) -> anyhow::Result<GitAdapterRun> {
    let target = parse_git_target(&input.target_url)
        .map_err(|err| anyhow::anyhow!("invalid git target: {}", err.message))?;
    let source_id = git_source_id(&target);
    let adapter = git_adapter_ref();
    let scope = SourceScope::Repo;
    let plan = source_plan(input, &target, &source_id, adapter.clone(), scope);
    Ok(GitAdapterRun {
        repo_root: input.repo_root.clone(),
        target,
        source_id,
        adapter,
        scope,
        plan,
    })
}

pub(super) async fn discover_manifest(run: &GitAdapterRun) -> anyhow::Result<SourceManifest> {
    Ok(GitSourceAdapter::new().discover(&run.plan).await?)
}

/// Output of [`normalize_changed_documents`]: the normalized documents plus
/// any enrichment-stage graph candidates keyed by `source_item_key`, for
/// [`super::git_source_vectorize::prepare_changed_documents`] to fold into
/// the matching document's `PrepareSourceDocumentRequest.graph_candidates`
/// (bypassing `DocumentPreparer`'s self-parse for that document, per the
/// `SourceEnricher` contract).
#[derive(Debug, Default)]
pub(super) struct NormalizedGitDocuments {
    pub(super) documents: Vec<SourceDocument>,
    pub(super) graph_candidates_by_item: BTreeMap<SourceItemKey, Vec<GraphCandidate>>,
}

/// Drives `acquire` -> `enrich` (one call per acquired item, source-pipeline.md
/// `enriching` stage) -> `normalize`. The enrichment stage sits strictly
/// between acquire and normalize: it sees the raw `AcquiredSourceItem`s before
/// normalization discards fetch-time detail, and its `parse_hints` are merged
/// into the corresponding normalized `SourceDocument.parser_hints` (already an
/// existing field the parser step consults) while `graph_candidates` are
/// carried out-of-band for the caller to thread into `prepare`.
pub(super) async fn normalize_changed_documents(
    run: &GitAdapterRun,
    diff: &SourceManifestDiff,
    enricher: &dyn SourceEnricher,
) -> anyhow::Result<NormalizedGitDocuments> {
    let acquisition = GitSourceAdapter::new().acquire(&run.plan, diff).await?;
    let enrichments = enrich_fetched_items(run, enricher, &acquisition.fetched_items).await?;
    let mut documents = GitSourceAdapter::new()
        .normalize(&run.plan, acquisition)
        .await?
        .data;
    let mut graph_candidates_by_item = BTreeMap::new();
    for document in &mut documents {
        let Some(enrichment) = enrichments.get(&document.source_item_key) else {
            continue;
        };
        document.parser_hints.extend(enrichment.parse_hints.clone());
        if !enrichment.graph_candidates.is_empty() {
            graph_candidates_by_item.insert(
                document.source_item_key.clone(),
                enrichment.graph_candidates.clone(),
            );
        }
    }
    Ok(NormalizedGitDocuments {
        documents,
        graph_candidates_by_item,
    })
}

/// Enrich every acquired item and return the results keyed by
/// `source_item_key`. With `NoopSourceEnricher` (the production default) this
/// is a no-op passthrough — every call returns `EnrichmentStatus::NotNeeded`
/// with empty hints/candidates.
async fn enrich_fetched_items(
    run: &GitAdapterRun,
    enricher: &dyn SourceEnricher,
    fetched_items: &[AcquiredSourceItem],
) -> anyhow::Result<BTreeMap<SourceItemKey, SourceEnrichment>> {
    let mut enrichments = BTreeMap::new();
    for item in fetched_items {
        let enrichment = enricher.enrich(&run.plan, item).await.map_err(|err| {
            anyhow::anyhow!(
                "git source enrichment failed for {}: {}",
                item.manifest_item.source_item_key.0,
                err.message
            )
        })?;
        enrichments.insert(item.manifest_item.source_item_key.clone(), enrichment);
    }
    Ok(enrichments)
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

pub(super) fn source_summary(input: &GitSourceIndexInput, run: &GitAdapterRun) -> SourceSummary {
    SourceSummary {
        source_id: run.source_id.clone(),
        canonical_uri: run.target.web_url.clone(),
        display_name: run.target.repo.clone(),
        source_kind: SourceKind::Git,
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
        graph_node_ids: Vec::new(),
        last_refreshed_at: None,
        user_label: None,
        tags: vec![run.target.provider.clone()],
        watch_id: None,
        last_job_id: Some(input.job_id),
    }
}

pub(super) fn git_adapter_ref() -> AdapterRef {
    AdapterRef {
        name: "git".to_string(),
        version: GIT_ADAPTER_VERSION.to_string(),
    }
}

pub(crate) fn git_source_id(target: &GitTarget) -> SourceId {
    SourceId::new(format!("src_git_{}", stable_token(&target.web_url)))
}

pub(crate) fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}

pub(super) fn stable_token(value: &str) -> String {
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
    input: &GitSourceIndexInput,
    target: &GitTarget,
    source_id: &SourceId,
    adapter: AdapterRef,
    scope: SourceScope,
) -> SourcePlan {
    let route = routed_plan(input, target, source_id, &adapter, scope);
    SourcePlan {
        job_id: input.job_id,
        request: SourceRequest::new(input.target_url.clone()),
        route,
        stage_plan: Vec::new(),
        limits: EffectiveLimits {
            request: SourceLimits::default(),
            adapter_defaults: SourceLimits::default(),
            config_defaults: SourceLimits::default(),
            effective: SourceLimits::default(),
        },
        config_snapshot_id: ConfigSnapshotId::new("cfg_git_source"),
        provider_reservations: provider_reservations(input),
    }
}

/// Build the `RoutePlan` embedded in the git `SourcePlan`.
///
/// When `input.route` carries the real routed plan from
/// `source::routing::resolve_source_route` (S2-routeplan-threading), its
/// `validated_options`, `credential_requirements`, `provider_requirements`,
/// `safety_class`, and hint fields survive into acquisition — only the
/// runtime-resolved `source`/`adapter`/`scope` (known only once the clone is
/// on disk) are overlaid. Falls back to the pre-S2 ad-hoc `RoutePlan` when no
/// route was threaded (tests, direct bridge callers).
fn routed_plan(
    input: &GitSourceIndexInput,
    target: &GitTarget,
    source_id: &SourceId,
    adapter: &AdapterRef,
    scope: SourceScope,
) -> RoutePlan {
    let resolved_source = ResolvedSource {
        source: input.target_url.clone(),
        canonical_uri: target.web_url.clone(),
        source_id: source_id.clone(),
        source_kind: SourceKind::Git,
        adapter: adapter.clone(),
        default_scope: scope,
        available_scopes: vec![scope],
        authority: AuthorityLevel::UserPinned,
        confidence: 1.0,
        reason: "target git source".to_string(),
        graph: Vec::new(),
        warnings: Vec::new(),
        metadata: MetadataMap::new(),
    };

    if let Some(routed) = &input.route {
        return RoutePlan {
            source: resolved_source,
            adapter: adapter.clone(),
            scope,
            ..routed.clone()
        };
    }

    RoutePlan {
        source: resolved_source,
        adapter: adapter.clone(),
        scope,
        provider_requirements: Vec::new(),
        credential_requirements: Vec::new(),
        execution_affinity: ExecutionAffinity::Worker,
        safety_class: SafetyClass::LocalFilesystem,
        option_schema_id: "adapter:git:options:v1".to_string(),
        validated_options: AdapterOptions {
            values: adapter_options(input),
        },
        chunking_hints: Vec::new(),
        parser_hints: Vec::new(),
        graph_fact_kinds: Vec::new(),
        watch_supported: true,
        refresh_supported: true,
    }
}

fn provider_reservations(input: &GitSourceIndexInput) -> Vec<ProviderReservationRequest> {
    let mut reservations = Vec::new();
    if input.embedding_reservations.is_some() {
        reservations.push(ProviderReservationRequest {
            provider_kind: ProviderKind::Embedding,
            priority: JobPriority::Background,
            units: 1,
            reason: "git source embedding".to_string(),
        });
    }
    if input.vector_reservations.is_some() {
        reservations.push(ProviderReservationRequest {
            provider_kind: ProviderKind::Vector,
            priority: JobPriority::Background,
            units: 1,
            reason: "git source vector write".to_string(),
        });
    }
    reservations
}

fn adapter_options(input: &GitSourceIndexInput) -> MetadataMap {
    let mut options = MetadataMap::new();
    options.insert(
        "repo_root".to_string(),
        serde_json::json!(input.repo_root.to_string_lossy()),
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

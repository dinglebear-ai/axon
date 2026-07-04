use std::path::{Path, PathBuf};

use anyhow::Context;
use axon_adapters::{SourceAdapter, feed::FeedSourceAdapter};
use axon_api::source::*;
use sha2::{Digest, Sha256};

use super::{FEED_ADAPTER_VERSION, FeedSourceIndexInput};

#[derive(Debug, Clone)]
pub(super) struct FeedAdapterRun {
    pub feed_path: PathBuf,
    pub source_id: SourceId,
    pub source_token: String,
    pub adapter: AdapterRef,
    pub scope: SourceScope,
    pub plan: SourcePlan,
}

pub(super) async fn resolve_adapter_run(
    input: &FeedSourceIndexInput,
) -> anyhow::Result<FeedAdapterRun> {
    let feed_path = tokio::fs::canonicalize(&input.feed_path)
        .await
        .with_context(|| {
            format!(
                "invalid feed source path {}",
                public_path_hint(&input.feed_path)
            )
        })?;
    let source_token = source_token(&feed_path);
    let source_id = feed_source_id(&feed_path);
    let scope = SourceScope::Feed;
    let adapter = feed_adapter_ref();
    let plan = source_plan(input, &feed_path, &source_id, adapter.clone(), scope);
    Ok(FeedAdapterRun {
        feed_path,
        source_id,
        source_token,
        adapter,
        scope,
        plan,
    })
}

pub(super) async fn discover_manifest(run: &FeedAdapterRun) -> anyhow::Result<SourceManifest> {
    Ok(FeedSourceAdapter::new().discover(&run.plan).await?)
}

pub(super) async fn normalize_changed_documents(
    run: &FeedAdapterRun,
    diff: &SourceManifestDiff,
) -> anyhow::Result<Vec<SourceDocument>> {
    let acquisition = FeedSourceAdapter::new().acquire(&run.plan, diff).await?;
    Ok(FeedSourceAdapter::new()
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

pub(super) fn source_summary(input: &FeedSourceIndexInput, run: &FeedAdapterRun) -> SourceSummary {
    SourceSummary {
        source_id: run.source_id.clone(),
        canonical_uri: format!("feed://{}", run.source_token),
        display_name: run
            .feed_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("feed-source")
            .to_string(),
        source_kind: SourceKind::Feed,
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

pub(super) fn feed_adapter_ref() -> AdapterRef {
    AdapterRef {
        name: "feed".to_string(),
        version: FEED_ADAPTER_VERSION.to_string(),
    }
}

pub(crate) fn feed_source_id(feed_path: &Path) -> SourceId {
    SourceId::new(format!("src_feed_{}", source_token(feed_path)))
}

pub(super) fn source_token(feed_path: &Path) -> String {
    stable_token(&file_url_for_path(feed_path).unwrap_or_else(|_| feed_path.display().to_string()))
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
    input: &FeedSourceIndexInput,
    feed_path: &Path,
    source_id: &SourceId,
    adapter: AdapterRef,
    scope: SourceScope,
) -> SourcePlan {
    let requested_uri = feed_path.to_string_lossy().to_string();
    let canonical_uri = format!("feed://{}", source_token(feed_path));
    let source = SourceRequest::new(requested_uri.clone());
    SourcePlan {
        job_id: input.job_id,
        request: source,
        route: RoutePlan {
            source: ResolvedSource {
                source: requested_uri.clone(),
                canonical_uri: canonical_uri.clone(),
                source_id: source_id.clone(),
                source_kind: SourceKind::Feed,
                adapter: adapter.clone(),
                default_scope: scope,
                available_scopes: vec![scope],
                authority: AuthorityLevel::UserPinned,
                confidence: 1.0,
                reason: "target feed source".to_string(),
                graph: Vec::new(),
                warnings: Vec::new(),
                metadata: MetadataMap::new(),
            },
            adapter,
            scope,
            provider_requirements: Vec::new(),
            credential_requirements: Vec::new(),
            execution_affinity: ExecutionAffinity::Worker,
            safety_class: SafetyClass::LocalFilesystem,
            option_schema_id: "adapter:feed:options:v1".to_string(),
            validated_options: AdapterOptions {
                values: adapter_options(feed_path),
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
        config_snapshot_id: ConfigSnapshotId::new("cfg_feed_source"),
        provider_reservations: provider_reservations(input),
    }
}

fn provider_reservations(input: &FeedSourceIndexInput) -> Vec<ProviderReservationRequest> {
    let mut reservations = Vec::new();
    if input.embedding_reservations.is_some() {
        reservations.push(ProviderReservationRequest {
            provider_kind: ProviderKind::Embedding,
            priority: JobPriority::Background,
            units: 1,
            reason: "feed source embedding".to_string(),
        });
    }
    if input.vector_reservations.is_some() {
        reservations.push(ProviderReservationRequest {
            provider_kind: ProviderKind::Vector,
            priority: JobPriority::Background,
            units: 1,
            reason: "feed source vector write".to_string(),
        });
    }
    reservations
}

fn adapter_options(feed_path: &Path) -> MetadataMap {
    let mut options = MetadataMap::new();
    options.insert(
        "feed_path".to_string(),
        serde_json::json!(feed_path.to_string_lossy()),
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

fn file_url_for_path(path: &Path) -> anyhow::Result<String> {
    url::Url::from_file_path(path)
        .map(|url| url.to_string())
        .map_err(|()| anyhow::anyhow!("failed to build file URL for {}", public_path_hint(path)))
}

fn public_path_hint(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| "feed-source".to_string())
}

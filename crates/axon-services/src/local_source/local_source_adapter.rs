use std::path::{Path, PathBuf};

use anyhow::Context;
use axon_adapters::{SourceAdapter, local::LocalSourceAdapter};
use axon_api::source::*;
use sha2::{Digest, Sha256};

use super::{LOCAL_ADAPTER_VERSION, LocalSourceIndexInput, LocalSourceSelectionPolicy};

const DEFAULT_LOCAL_MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub(super) struct LocalAdapterRun {
    pub root: PathBuf,
    pub source_id: SourceId,
    pub source_token: String,
    pub adapter: AdapterRef,
    pub scope: SourceScope,
    pub plan: SourcePlan,
}

pub(super) async fn resolve_adapter_run(
    input: &LocalSourceIndexInput,
) -> anyhow::Result<LocalAdapterRun> {
    let root = tokio::fs::canonicalize(&input.root)
        .await
        .with_context(|| {
            format!(
                "invalid local source root {}",
                public_path_hint(&input.root)
            )
        })?;
    let root_is_file = tokio::fs::metadata(&root)
        .await
        .with_context(|| {
            format!(
                "failed to stat local source root {}",
                public_path_hint(&root)
            )
        })?
        .is_file();
    let source_token = source_token(&root);
    let source_id = local_source_id(&root);
    let scope = if root_is_file {
        SourceScope::File
    } else if input.selection_policy == LocalSourceSelectionPolicy::CodeSearch {
        SourceScope::Repo
    } else {
        SourceScope::Directory
    };
    let adapter = local_adapter_ref();
    let plan = source_plan(
        input,
        &root,
        &source_id,
        &source_token,
        adapter.clone(),
        scope,
    );
    Ok(LocalAdapterRun {
        root,
        source_id,
        source_token,
        adapter,
        scope,
        plan,
    })
}

pub(super) async fn discover_manifest(run: &LocalAdapterRun) -> anyhow::Result<SourceManifest> {
    Ok(LocalSourceAdapter::new().discover(&run.plan).await?)
}

pub(super) async fn normalize_changed_documents(
    run: &LocalAdapterRun,
    diff: &SourceManifestDiff,
) -> anyhow::Result<Vec<SourceDocument>> {
    let acquisition = LocalSourceAdapter::new().acquire(&run.plan, diff).await?;
    Ok(LocalSourceAdapter::new()
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
    input: &LocalSourceIndexInput,
    run: &LocalAdapterRun,
) -> SourceSummary {
    SourceSummary {
        source_id: run.source_id.clone(),
        canonical_uri: format!("local://{}", run.source_token),
        display_name: run
            .root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("local-source")
            .to_string(),
        source_kind: SourceKind::Local,
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

pub(super) fn local_adapter_ref() -> AdapterRef {
    AdapterRef {
        name: "local".to_string(),
        version: LOCAL_ADAPTER_VERSION.to_string(),
    }
}

pub(crate) fn local_source_id(root: &Path) -> SourceId {
    SourceId::new(format!("src_local_{}", source_token(root)))
}

pub(super) fn source_token(root: &Path) -> String {
    stable_token(&file_url_for_path(root).unwrap_or_else(|_| root.display().to_string()))
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
    input: &LocalSourceIndexInput,
    root: &Path,
    source_id: &SourceId,
    source_token: &str,
    adapter: AdapterRef,
    scope: SourceScope,
) -> SourcePlan {
    let canonical_uri = format!("local://{source_token}");
    let source = SourceRequest::local_path(root.to_string_lossy().to_string(), !root.is_file());
    SourcePlan {
        job_id: input.job_id,
        request: source,
        route: RoutePlan {
            source: ResolvedSource {
                requested_uri: root.to_string_lossy().to_string(),
                canonical_uri: canonical_uri.clone(),
                source_id: source_id.clone(),
                source_kind: SourceKind::Local,
                display_name: public_path_hint(root),
                candidate_adapters: vec![AdapterCandidate {
                    adapter: adapter.clone(),
                    supported_scopes: vec![scope],
                    confidence: 1.0,
                    reason: "target local source".to_string(),
                }],
                default_scope: scope,
                available_scopes: vec![scope],
                authority: AuthorityLevel::UserPinned,
                confidence: 1.0,
                reason: "target local source".to_string(),
                authority_hint: None,
                warnings: Vec::new(),
            },
            adapter,
            scope,
            provider_requirements: Vec::new(),
            credential_requirements: Vec::new(),
            execution_affinity: ExecutionAffinity::Worker,
            safety_class: SafetyClass::LocalFilesystem,
            option_schema_id: "adapter:local:options:v1".to_string(),
            validated_options: AdapterOptions {
                values: adapter_options(input.selection_policy),
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
        config_snapshot_id: ConfigSnapshotId::new("cfg_local_source"),
        provider_reservations: provider_reservations(input),
    }
}

fn provider_reservations(input: &LocalSourceIndexInput) -> Vec<ProviderReservationRequest> {
    let mut reservations = Vec::new();
    if input.embedding_reservations.is_some() {
        reservations.push(ProviderReservationRequest {
            provider_kind: ProviderKind::Embedding,
            priority: JobPriority::Background,
            units: 1,
            reason: "local source embedding".to_string(),
        });
    }
    if input.vector_reservations.is_some() {
        reservations.push(ProviderReservationRequest {
            provider_kind: ProviderKind::Vector,
            priority: JobPriority::Background,
            units: 1,
            reason: "local source vector write".to_string(),
        });
    }
    reservations
}

fn adapter_options(selection_policy: LocalSourceSelectionPolicy) -> MetadataMap {
    let mut options = MetadataMap::new();
    options.insert(
        "max_file_bytes".to_string(),
        serde_json::json!(DEFAULT_LOCAL_MAX_FILE_BYTES),
    );
    if selection_policy == LocalSourceSelectionPolicy::CodeSearch {
        options.insert("respect_gitignore".to_string(), serde_json::json!(true));
    }
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
        .unwrap_or_else(|| "local-source".to_string())
}

use std::path::Path;

use axon_adapters::{SourceAdapter, registry_sources::RegistrySourceAdapter};
use axon_api::source::*;
use sha2::{Digest, Sha256};

use super::{REGISTRY_ADAPTER_VERSION, RegistrySourceIndexInput};

#[derive(Debug, Clone)]
pub(super) struct RegistryAdapterRun {
    pub source_id: SourceId,
    pub source_token: String,
    pub adapter: AdapterRef,
    pub scope: SourceScope,
    pub plan: SourcePlan,
}

pub(super) fn resolve_adapter_run(
    input: &RegistrySourceIndexInput,
) -> anyhow::Result<RegistryAdapterRun> {
    let dump_path_display = public_path_hint(&input.registry_dump_path);
    let source_token = source_token(&input.registry_dump_path);
    let source_id = registry_source_id(&input.registry_dump_path);
    let scope = SourceScope::Package;
    let adapter = registry_adapter_ref();
    let plan = source_plan(
        input,
        &dump_path_display,
        &source_id,
        &source_token,
        adapter.clone(),
        scope,
    );
    Ok(RegistryAdapterRun {
        source_id,
        source_token,
        adapter,
        scope,
        plan,
    })
}

pub(super) async fn discover_manifest(run: &RegistryAdapterRun) -> anyhow::Result<SourceManifest> {
    Ok(RegistrySourceAdapter::new().discover(&run.plan).await?)
}

pub(super) async fn normalize_changed_documents(
    run: &RegistryAdapterRun,
    diff: &SourceManifestDiff,
) -> anyhow::Result<Vec<SourceDocument>> {
    let adapter = RegistrySourceAdapter::new();
    let acquisition = adapter.acquire(&run.plan, diff).await?;
    let documents = adapter.normalize(&run.plan, acquisition).await?.data;
    Ok(documents
        .into_iter()
        .map(remap_to_vector_payload_contract)
        .collect())
}

/// `axon-adapters::registry_sources` stamps document metadata using the
/// legacy `pkg_*` field names and a `source_family = "registry"` value that
/// mirror the pre-unification npm/pypi vertical extractors (see
/// `crates/axon-adapters/src/registry_sources/metadata.rs`). Those verticals
/// wrote payloads directly and were never bound by the shared vector payload
/// contract in `axon-vectors::payload`, which only recognizes the
/// `"package"` source family with `package_ecosystem` / `package_name` /
/// `package_version` as its source-specific fields — any other field is
/// rejected by `VectorPointBatchBuilder::build()`. Remap here, at the bridge
/// boundary, rather than editing the already-merged adapter contract: this
/// keeps the fix scoped to the one crate allowed to reach into domain
/// internals (`axon-services`) and leaves the adapter's own unit tests (which
/// assert the pre-remap `pkg_*` shape) untouched.
fn remap_to_vector_payload_contract(mut document: SourceDocument) -> SourceDocument {
    let ecosystem = document.metadata.remove("pkg_registry");
    let name = document.metadata.remove("pkg_name");
    let version = document.metadata.remove("pkg_version");
    document.metadata.remove("pkg_license");
    document.metadata.remove("pkg_author");
    document.metadata.remove("pkg_keywords");
    document.metadata.remove("pkg_homepage");
    document
        .metadata
        .insert("source_family".to_string(), serde_json::json!("package"));
    if let Some(ecosystem) = ecosystem {
        document
            .metadata
            .insert("package_ecosystem".to_string(), ecosystem);
    }
    if let Some(name) = name {
        document.metadata.insert("package_name".to_string(), name);
    }
    if let Some(version) = version {
        document
            .metadata
            .insert("package_version".to_string(), version);
    }
    document
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
    input: &RegistrySourceIndexInput,
    run: &RegistryAdapterRun,
) -> SourceSummary {
    SourceSummary {
        source_id: run.source_id.clone(),
        canonical_uri: format!("registry://{}", run.source_token),
        display_name: input
            .registry_dump_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("registry-source")
            .to_string(),
        source_kind: SourceKind::Registry,
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
        tags: vec![format!("{:?}", run.scope).to_ascii_lowercase()],
        watch_id: None,
        last_job_id: Some(input.job_id),
    }
}

pub(super) fn registry_adapter_ref() -> AdapterRef {
    AdapterRef {
        name: "registry".to_string(),
        version: REGISTRY_ADAPTER_VERSION.to_string(),
    }
}

pub(crate) fn registry_source_id(dump_path: &Path) -> SourceId {
    SourceId::new(format!("src_registry_{}", source_token(dump_path)))
}

pub(super) fn source_token(dump_path: &Path) -> String {
    stable_token(&dump_path.display().to_string())
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
    input: &RegistrySourceIndexInput,
    dump_path_display: &str,
    source_id: &SourceId,
    source_token: &str,
    adapter: AdapterRef,
    scope: SourceScope,
) -> SourcePlan {
    let canonical_uri = format!("registry://{source_token}");
    let source = SourceRequest::new(dump_path_display.to_string());
    SourcePlan {
        job_id: input.job_id,
        request: source,
        route: RoutePlan {
            source: ResolvedSource {
                source: dump_path_display.to_string(),
                canonical_uri: canonical_uri.clone(),
                source_id: source_id.clone(),
                source_kind: SourceKind::Registry,
                adapter: adapter.clone(),
                default_scope: scope,
                available_scopes: vec![scope],
                authority: AuthorityLevel::UserPinned,
                confidence: 1.0,
                reason: "target registry source".to_string(),
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
            option_schema_id: "adapter:registry:options:v1".to_string(),
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
        config_snapshot_id: ConfigSnapshotId::new("cfg_registry_source"),
        provider_reservations: provider_reservations(input),
    }
}

fn provider_reservations(input: &RegistrySourceIndexInput) -> Vec<ProviderReservationRequest> {
    let mut reservations = Vec::new();
    if input.embedding_reservations.is_some() {
        reservations.push(ProviderReservationRequest {
            provider_kind: ProviderKind::Embedding,
            priority: JobPriority::Background,
            units: 1,
            reason: "registry source embedding".to_string(),
        });
    }
    if input.vector_reservations.is_some() {
        reservations.push(ProviderReservationRequest {
            provider_kind: ProviderKind::Vector,
            priority: JobPriority::Background,
            units: 1,
            reason: "registry source vector write".to_string(),
        });
    }
    reservations
}

fn adapter_options(input: &RegistrySourceIndexInput) -> MetadataMap {
    let mut options = MetadataMap::new();
    options.insert(
        "registry_dump_path".to_string(),
        serde_json::json!(input.registry_dump_path.display().to_string()),
    );
    if input.include_all_versions {
        options.insert("include_all_versions".to_string(), serde_json::json!(true));
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

fn public_path_hint(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| "registry-source".to_string())
}

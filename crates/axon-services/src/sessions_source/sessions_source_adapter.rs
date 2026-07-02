use std::path::{Path, PathBuf};

use anyhow::Context;
use axon_adapters::SourceAdapter;
use axon_adapters::sessions::SessionSourceAdapter;
use axon_api::source::*;
use axon_vectors::payload::VECTOR_SHARED_FIELDS;
use sha2::{Digest, Sha256};

use super::{SESSIONS_ADAPTER_VERSION, SessionsSourceIndexInput};

#[derive(Debug, Clone)]
pub(super) struct SessionsAdapterRun {
    pub sessions_root: PathBuf,
    pub source_id: SourceId,
    pub adapter: AdapterRef,
    pub plan: SourcePlan,
}

pub(super) fn resolve_adapter_run(
    input: &SessionsSourceIndexInput,
) -> anyhow::Result<SessionsAdapterRun> {
    let sessions_root = input.sessions_root.clone();
    let root_is_file = std::fs::metadata(&sessions_root)
        .with_context(|| {
            format!(
                "failed to stat sessions source root {}",
                public_path_hint(&sessions_root)
            )
        })?
        .is_file();
    let source_token = source_token(&input.provider, &input.session_id);
    let source_id = sessions_source_id(&input.provider, &input.session_id);
    let scope = if root_is_file {
        SourceScope::File
    } else {
        SourceScope::Thread
    };
    let adapter = sessions_adapter_ref();
    let plan = source_plan(input, &source_id, &source_token, adapter.clone(), scope);
    Ok(SessionsAdapterRun {
        sessions_root,
        source_id,
        adapter,
        plan,
    })
}

pub(super) async fn discover_manifest(run: &SessionsAdapterRun) -> anyhow::Result<SourceManifest> {
    Ok(SessionSourceAdapter::new().discover(&run.plan).await?)
}

pub(super) async fn normalize_changed_documents(
    run: &SessionsAdapterRun,
    diff: &SourceManifestDiff,
) -> anyhow::Result<Vec<SourceDocument>> {
    let adapter = SessionSourceAdapter::new();
    let acquisition = adapter.acquire(&run.plan, diff).await?;
    let documents = adapter.normalize(&run.plan, acquisition).await?.data;
    Ok(documents
        .into_iter()
        .map(remap_to_vector_payload_contract)
        .collect())
}

/// `axon-adapters::sessions` stamps a rich set of `session_*` metadata fields
/// (`session_provider`, `session_agent`, `session_turn_count`,
/// `session_has_tool_use`, `session_tools_used`, `session_model`,
/// `session_workspace_path`, `session_git_branch`, `session_last_message_at`)
/// that mirror the legacy `axon-ingest::sessions` transcript payloads. Those
/// verticals wrote payloads directly and were never bound by the shared vector
/// payload contract in `axon-vectors::payload`, which for the `"session"`
/// source family recognizes only `session_id` / `session_turn_index` /
/// `session_tool_name` / `session_skill_name` as source-specific fields — any
/// other field is rejected by `VectorPointBatchBuilder::build()` with
/// `UnknownSourceSpecificField`. Worse, `session_workspace_path` carries an
/// absolute filesystem path (e.g. `/home/user/project`), which the redaction
/// guard in `axon-vectors::payload_redaction` rejects outright as a
/// `ForbiddenValue`.
///
/// Remap here, at the bridge boundary, rather than editing the already-merged
/// adapter contract: this keeps the fix scoped to the one crate allowed to
/// reach into domain internals (`axon-services`) and leaves the adapter's own
/// unit tests (which assert the full pre-remap `session_*` shape) untouched.
/// We drop every session-specific field the contract does not allow (including
/// the forbidden `session_workspace_path`) while preserving the contract's
/// shared fields and the allowlisted `session_id`.
fn remap_to_vector_payload_contract(mut document: SourceDocument) -> SourceDocument {
    document.metadata.retain(|field, _| {
        VECTOR_SHARED_FIELDS.contains(&field.as_str())
            || SESSION_PAYLOAD_ALLOWED_FIELDS.contains(&field.as_str())
    });
    document
}

/// Session source-specific fields accepted by the shared vector payload
/// contract (`axon-vectors::payload::VECTOR_SOURCE_FAMILY_FIELDS`, `"session"`
/// row). Kept in sync with that table; every other stamped `session_*` field
/// is dropped by [`remap_to_vector_payload_contract`].
const SESSION_PAYLOAD_ALLOWED_FIELDS: &[&str] = &[
    "session_id",
    "session_turn_index",
    "session_tool_name",
    "session_skill_name",
];

/// Strip prepared-chunk metadata fields the shared vector payload contract does
/// not accept for the session family. `DocumentPreparer` stamps `segment_kind`
/// on every transcript chunk (see `axon-document::transcript`); that field is
/// not in the contract's session allowlist and is not filtered by the
/// preparer-internal chunk-metadata carve-out, so it would trip
/// `UnknownSourceSpecificField` when the chunk metadata is merged into the
/// point payload. Sanitize at the bridge boundary using the same allowlist as
/// [`remap_to_vector_payload_contract`], keeping the document-level and
/// chunk-level payload contracts in lockstep.
pub(super) fn sanitize_prepared_chunk_metadata(document: &mut PreparedDocument) {
    for chunk in &mut document.chunks {
        chunk.metadata.retain(|field, _| {
            VECTOR_SHARED_FIELDS.contains(&field.as_str())
                || SESSION_PAYLOAD_ALLOWED_FIELDS.contains(&field.as_str())
        });
    }
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
    input: &SessionsSourceIndexInput,
    run: &SessionsAdapterRun,
) -> SourceSummary {
    SourceSummary {
        source_id: run.source_id.clone(),
        canonical_uri: format!("session://{}/{}", input.provider, input.session_id),
        display_name: run
            .sessions_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("sessions-source")
            .to_string(),
        source_kind: SourceKind::Session,
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
        tags: vec![input.provider.clone()],
        watch_id: None,
        last_job_id: Some(input.job_id),
    }
}

pub(super) fn sessions_adapter_ref() -> AdapterRef {
    AdapterRef {
        name: "session".to_string(),
        version: SESSIONS_ADAPTER_VERSION.to_string(),
    }
}

pub(crate) fn sessions_source_id(provider: &str, session_id: &str) -> SourceId {
    SourceId::new(format!(
        "src_session_{}",
        source_token(provider, session_id)
    ))
}

pub(super) fn source_token(provider: &str, session_id: &str) -> String {
    stable_token(&format!("session:{provider}:{session_id}"))
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
    input: &SessionsSourceIndexInput,
    source_id: &SourceId,
    source_token: &str,
    adapter: AdapterRef,
    scope: SourceScope,
) -> SourcePlan {
    let canonical_uri = format!("session://{}/{}", input.provider, input.session_id);
    let request_source = format!("session:{}:{}", input.provider, input.session_id);
    let mut values = MetadataMap::new();
    values.insert(
        "sessions_root".to_string(),
        serde_json::json!(input.sessions_root.to_string_lossy()),
    );
    let mut request = SourceRequest::new(request_source.clone());
    request.adapter = Some("session".to_string());
    request.scope = Some(scope);
    SourcePlan {
        job_id: input.job_id,
        request,
        route: RoutePlan {
            source: ResolvedSource {
                requested_uri: request_source.clone(),
                canonical_uri: canonical_uri.clone(),
                source_id: source_id.clone(),
                source_kind: SourceKind::Session,
                display_name: public_path_hint(&input.sessions_root),
                candidate_adapters: vec![AdapterCandidate {
                    adapter: adapter.clone(),
                    supported_scopes: vec![scope],
                    confidence: 1.0,
                    reason: "target sessions source".to_string(),
                }],
                default_scope: scope,
                available_scopes: vec![scope],
                authority: AuthorityLevel::UserPinned,
                confidence: 1.0,
                reason: "target sessions source".to_string(),
                authority_hint: None,
                warnings: Vec::new(),
            },
            adapter,
            scope,
            provider_requirements: Vec::new(),
            credential_requirements: Vec::new(),
            execution_affinity: ExecutionAffinity::Worker,
            safety_class: SafetyClass::LocalFilesystem,
            option_schema_id: "adapter:session:options:v1".to_string(),
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
        config_snapshot_id: ConfigSnapshotId::new(format!("cfg_sessions_source_{source_token}")),
        provider_reservations: provider_reservations(input),
    }
}

fn provider_reservations(input: &SessionsSourceIndexInput) -> Vec<ProviderReservationRequest> {
    let mut reservations = Vec::new();
    if input.embedding_reservations.is_some() {
        reservations.push(ProviderReservationRequest {
            provider_kind: ProviderKind::Embedding,
            priority: JobPriority::Background,
            units: 1,
            reason: "sessions source embedding".to_string(),
        });
    }
    if input.vector_reservations.is_some() {
        reservations.push(ProviderReservationRequest {
            provider_kind: ProviderKind::Vector,
            priority: JobPriority::Background,
            units: 1,
            reason: "sessions source vector write".to_string(),
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

fn public_path_hint(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| "sessions-source".to_string())
}

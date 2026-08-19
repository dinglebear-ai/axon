//! AI session transcript source adapter (Claude / Codex / Gemini exports).
//!
//! The adapter owns provider-root selection and validation. Materialization
//! resolves the session selector against approved agent roots and stamps the
//! validated file or directory on the routed plan before discovery.
//!
//! Format detection is by file extension: `claude` and `codex` sessions are
//! JSONL (one JSON object per line); `gemini` sessions are a single JSON
//! document. The provider itself comes from the routed `session:<provider>:<id>`
//! target, not from sniffing file content — the router / caller already knows
//! which agent produced the export it is handing to this adapter.

mod decode;
mod metadata;
mod project_filter;
mod selection;
mod target;

use std::collections::BinaryHeap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use axon_api::source::*;
use ignore::{DirEntry, WalkBuilder, WalkState};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::adapter::{Result, SourceAdapter};
use crate::capability::AdapterCapability;
use crate::manifest::item_identity;

use self::decode::DecodedSession;
pub use self::decode::redact_session_text;
use self::metadata::session_source_document;
use self::project_filter::matches_project_filter;
pub use self::selection::{
    SessionProvider, SessionRoots, ValidatedSessionPath, has_supported_session_extension,
    validate_event_path_missing_ok, validate_session_file_path, validate_session_source_path,
    validate_session_source_path_async,
};
pub use self::target::{SessionTarget, parse_session_target};

pub const MODULE_NAME: &str = "sessions";

const ADAPTER_NAME: &str = "sessions";
const SESSION_DISCOVERY_HASH_MAX_THREADS: usize = 8;
// Manifest freshness for the normalized semantic document projection. Bump
// when decoding, redaction, or document construction changes so an unchanged
// raw transcript is re-prepared instead of silently reusing stale vectors.
const SESSION_DOCUMENT_VERSION: &str = "semantic-v3";

#[derive(Debug, Clone, Default)]
pub struct SessionSourceAdapter;

impl SessionSourceAdapter {
    pub fn new() -> Self {
        Self
    }

    pub async fn materialize_with_roots(
        &self,
        mut plan: SourcePlan,
        roots: &SessionRoots,
    ) -> Result<crate::acquisition::MaterializedSource> {
        validate_adapter(&plan)?;
        let target = session_target(&plan)?;
        let provider = SessionProvider::parse(&target.provider).map_err(|err| {
            crate::acquisition::materialization_error(
                "adapter.session.provider_invalid",
                err.to_string(),
            )
        })?;
        let path =
            validate_session_source_path_async(roots, provider, Path::new(&target.session_id))
                .await
                .map_err(|err| {
                    crate::acquisition::materialization_error(
                        "adapter.session.selection_denied",
                        err.to_string(),
                    )
                })?;
        plan.route
            .validated_options
            .values
            .insert("sessions_root".to_string(), json!(path.to_string_lossy()));
        Ok(crate::acquisition::MaterializedSource::persistent(
            plan, path,
        ))
    }
}

#[async_trait]
impl SourceAdapter for SessionSourceAdapter {
    fn name(&self) -> &'static str {
        ADAPTER_NAME
    }

    fn version(&self) -> &'static str {
        crate::adapter::SOURCE_ADAPTER_CONTRACT_VERSION
    }

    async fn capabilities(&self) -> Result<SourceAdapterCapability> {
        Ok(session_capability(self.version()).into())
    }

    async fn materialize(
        &self,
        plan: SourcePlan,
    ) -> Result<crate::acquisition::MaterializedSource> {
        let roots = SessionRoots::from_home_env().map_err(|err| {
            crate::acquisition::materialization_error(
                "adapter.session.roots_unavailable",
                err.to_string(),
            )
        })?;
        self.materialize_with_roots(plan, &roots).await
    }

    async fn discover(&self, plan: &SourcePlan) -> Result<SourceManifest> {
        let plan = plan.clone();
        tokio::task::spawn_blocking(move || discover_sync(&plan))
            .await
            .map_err(blocking_join_error)?
    }

    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &SourceManifestDiff,
    ) -> Result<SourceAcquisition> {
        let plan = plan.clone();
        let diff = diff.clone();
        tokio::task::spawn_blocking(move || acquire_sync(&plan, &diff))
            .await
            .map_err(blocking_join_error)?
    }

    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquisition: SourceAcquisition,
    ) -> Result<StageExecutionResult<Vec<SourceDocument>>> {
        validate_adapter(plan)?;
        let target = session_target(plan)?;
        let SourceAcquisition {
            source_id,
            fetched_items,
            ..
        } = acquisition;
        let mut documents = Vec::with_capacity(fetched_items.len());
        for item in fetched_items {
            let AcquiredSourceItem {
                manifest_item,
                content_ref,
                raw_artifact_id,
                ..
            } = item;
            let text = item_text(content_ref)?;
            let decoded = decode_manifest_item(&target, &manifest_item, &text)?;
            documents.push(session_source_document(
                plan,
                &target,
                decoded,
                &source_id,
                manifest_item,
                raw_artifact_id,
            ));
        }
        Ok(StageExecutionResult {
            header: stage_header(
                plan.job_id,
                "session_normalize",
                PipelinePhase::Normalizing,
                documents.len(),
            ),
            data: documents,
        })
    }
}

fn session_capability(version: &str) -> AdapterCapability {
    AdapterCapability::new(
        AdapterRef {
            name: ADAPTER_NAME.to_string(),
            version: version.to_string(),
        },
        SourceKind::Session,
        SourceScope::File,
    )
    .with_scope(SourceScope::Directory)
    .with_scope(SourceScope::Thread)
}

fn discover_sync(plan: &SourcePlan) -> Result<SourceManifest> {
    session_capability(crate::adapter::SOURCE_ADAPTER_CONTRACT_VERSION)
        .validate_scope(plan.route.scope)?;
    validate_adapter(plan)?;
    let target = session_target(plan)?;
    let root = sessions_root(plan)?;
    let project_filter = project_filter(plan);

    let base_uri = format!("session://{}/{}", target.provider, target.session_id);
    let max_items = plan
        .limits
        .effective
        .max_items
        .map(|value| usize::try_from(value).unwrap_or(usize::MAX));
    let mut items = if let Some(limit) = max_items {
        let candidates =
            collect_capped_session_candidates(&target, project_filter.as_deref(), &root, limit)?;
        hash_session_candidates_parallel(plan, &base_uri, &root, &candidates)?
    } else {
        collect_session_manifest_items_parallel(
            plan,
            &target,
            project_filter.as_deref(),
            &base_uri,
            &root,
        )?
    };
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));

    Ok(SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: SourceGenerationId::from("gen_session_discovery"),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items,
        created_at: timestamp(),
        metadata: manifest_metadata(&target),
    })
}

fn acquire_sync(plan: &SourcePlan, diff: &SourceManifestDiff) -> Result<SourceAcquisition> {
    validate_adapter(plan)?;
    let root = sessions_root(plan)?;
    let manifest_items = diff
        .added
        .iter()
        .chain(diff.modified.iter())
        .cloned()
        .collect::<Vec<_>>();
    let mut fetched_items = Vec::with_capacity(manifest_items.len());
    for item in &manifest_items {
        let key = item
            .display_path
            .clone()
            .unwrap_or_else(|| item.source_item_key.0.clone());
        let path = safe_item_path(&root, &key)?;
        let text = fs::read_to_string(&path).map_err(|err| fs_error("read_failed", &path, err))?;
        fetched_items.push(AcquiredSourceItem {
            manifest_item: item.clone(),
            fetch_status: LifecycleStatus::Completed,
            content_ref: ContentRef::InlineText { text },
            raw_artifact_id: None,
            headers: RedactedHeaders {
                headers: Vec::new(),
            },
            fetched_at: timestamp(),
            metadata: MetadataMap::new(),
        });
    }

    let target = session_target(plan)?;
    let manifest = SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: diff.next_generation.clone(),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items: manifest_items,
        created_at: timestamp(),
        metadata: manifest_metadata(&target),
    };
    Ok(SourceAcquisition {
        header: stage_header(
            plan.job_id,
            "session_fetch",
            PipelinePhase::Fetching,
            fetched_items.len(),
        ),
        source_id: manifest.source_id.clone(),
        generation: manifest.generation.clone(),
        adapter: manifest.adapter.clone(),
        scope: manifest.scope,
        manifest,
        fetched_items,
        artifacts: Vec::new(),
    })
}

fn manifest_metadata(target: &SessionTarget) -> MetadataMap {
    let mut metadata = MetadataMap::new();
    metadata.insert("session_provider".to_string(), json!(target.provider));
    metadata.insert("session_id".to_string(), json!(target.session_id));
    metadata
}

/// The prepared export root, passed by the services bridge as a validated option.
/// May point at a directory of transcript files or a single file.
fn sessions_root(plan: &SourcePlan) -> Result<PathBuf> {
    plan.route
        .validated_options
        .values
        .get("sessions_root")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| {
            ApiError::new(
                "adapter.session.sessions_root.required",
                ErrorStage::Planning,
                "session adapter requires a sessions_root option pointing at prepared export files",
            )
        })
}

fn project_filter(plan: &SourcePlan) -> Option<String> {
    plan.route
        .validated_options
        .values
        .get("project_filter")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn session_target(plan: &SourcePlan) -> Result<SessionTarget> {
    parse_session_target(&plan.request.source)
}

fn validate_adapter(plan: &SourcePlan) -> Result<()> {
    if plan.route.adapter.name == ADAPTER_NAME {
        return Ok(());
    }
    Err(ApiError::new(
        "adapter.session.mismatch",
        ErrorStage::Routing,
        "route selected a different adapter",
    )
    .with_context("adapter", plan.route.adapter.name.clone()))
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
struct SessionFileCandidate {
    key: String,
}

fn session_walk_builder(root: &Path) -> WalkBuilder {
    let mut builder = WalkBuilder::new(root);
    builder
        .follow_links(false)
        .hidden(false)
        .git_ignore(false)
        .git_exclude(false)
        .parents(false)
        .filter_entry(should_descend_entry);
    builder
}

fn session_hash_threads(work_items: usize) -> usize {
    let cpus = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4);
    cpus.div_ceil(2)
        .clamp(1, SESSION_DISCOVERY_HASH_MAX_THREADS)
        .min(work_items.max(1))
}

fn collect_capped_session_candidates(
    target: &SessionTarget,
    project_filter: Option<&str>,
    root: &Path,
    limit: usize,
) -> Result<Vec<SessionFileCandidate>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut selected = BinaryHeap::with_capacity(limit.min(4096));
    for entry in session_walk_builder(root).build() {
        let entry = entry.map_err(session_walk_error)?;
        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            continue;
        }
        let path = entry.into_path();
        if !target_has_supported_session_extension(target, &path) {
            continue;
        }
        let key = relative_key(root, &path)?;
        if !matches_project_filter(project_filter, root, &path, &key) {
            continue;
        }
        let candidate = SessionFileCandidate { key };
        if selected.len() < limit {
            selected.push(candidate);
        } else if selected.peek().is_some_and(|largest| candidate < *largest) {
            selected.pop();
            selected.push(candidate);
        }
    }
    let mut selected = selected.into_vec();
    selected.sort();
    Ok(selected)
}

fn hash_session_candidates_parallel(
    plan: &SourcePlan,
    base_uri: &str,
    root: &Path,
    candidates: &[SessionFileCandidate],
) -> Result<Vec<ManifestItem>> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let threads = session_hash_threads(candidates.len());
    let chunk_size = candidates.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(threads);
        for chunk in candidates.chunks(chunk_size) {
            handles.push(scope.spawn(move || -> Result<Vec<ManifestItem>> {
                let mut items = Vec::with_capacity(chunk.len());
                for candidate in chunk {
                    if let Some(item) = session_manifest_item(plan, base_uri, root, &candidate.key)?
                    {
                        items.push(item);
                    }
                }
                Ok(items)
            }));
        }
        let mut items = Vec::with_capacity(candidates.len());
        for handle in handles {
            let batch = handle.join().map_err(|_| {
                ApiError::new(
                    "adapter.session.hash_worker_failed",
                    ErrorStage::Discovering,
                    "session discovery hash worker panicked",
                )
            })??;
            items.extend(batch);
        }
        Ok(items)
    })
}

fn collect_session_manifest_items_parallel(
    plan: &SourcePlan,
    target: &SessionTarget,
    project_filter: Option<&str>,
    base_uri: &str,
    root: &Path,
) -> Result<Vec<ManifestItem>> {
    let mut builder = session_walk_builder(root);
    builder.threads(session_hash_threads(usize::MAX));
    let items = Mutex::new(Vec::new());
    let first_error = Mutex::new(None);

    builder.build_parallel().run(|| {
        Box::new(|entry| {
            if first_error
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .is_some()
            {
                return WalkState::Quit;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    record_session_parallel_error(&first_error, session_walk_error(err));
                    return WalkState::Quit;
                }
            };
            if !entry
                .file_type()
                .is_some_and(|file_type| file_type.is_file())
            {
                return WalkState::Continue;
            }
            let path = entry.into_path();
            if !target_has_supported_session_extension(target, &path) {
                return WalkState::Continue;
            }
            let key = match relative_key(root, &path) {
                Ok(key) => key,
                Err(err) => {
                    record_session_parallel_error(&first_error, err);
                    return WalkState::Quit;
                }
            };
            if !matches_project_filter(project_filter, root, &path, &key) {
                return WalkState::Continue;
            }
            match session_manifest_item(plan, base_uri, root, &key) {
                Ok(Some(item)) => items
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .push(item),
                Ok(None) => {}
                Err(err) => {
                    record_session_parallel_error(&first_error, err);
                    return WalkState::Quit;
                }
            }
            WalkState::Continue
        })
    });

    if let Some(error) = first_error
        .into_inner()
        .unwrap_or_else(|poison| poison.into_inner())
    {
        return Err(error);
    }
    let mut items = items
        .into_inner()
        .unwrap_or_else(|poison| poison.into_inner());
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));
    Ok(items)
}

fn session_manifest_item(
    plan: &SourcePlan,
    base_uri: &str,
    root: &Path,
    key: &str,
) -> Result<Option<ManifestItem>> {
    let path = safe_item_path(root, key)?;
    let meta = fs::metadata(&path).map_err(|err| fs_error("stat_failed", &path, err))?;
    if !meta.is_file() {
        return Ok(None);
    }
    let content_hash = content_fingerprint(&path)?;
    let identity = item_identity(SourceKind::Session, base_uri, key)?;
    let mut item_metadata = MetadataMap::new();
    item_metadata.insert("session_relative_path".to_string(), json!(key));
    Ok(Some(ManifestItem {
        source_id: plan.route.source.source_id.clone(),
        source_item_key: identity.source_item_key,
        canonical_uri: identity.canonical_uri,
        item_kind: ItemKind::Transcript,
        content_kind: Some(ContentKind::Transcript),
        display_path: Some(key.to_string()),
        parent_key: None,
        size_bytes: Some(meta.len()),
        content_hash: Some(content_hash),
        mtime: modified_at(meta.modified().ok()),
        version: Some(SESSION_DOCUMENT_VERSION.to_string()),
        fetch_plan: None,
        metadata: item_metadata,
        graph_hints: Vec::new(),
    }))
}

fn record_session_parallel_error(slot: &Mutex<Option<ApiError>>, error: ApiError) {
    let mut slot = slot.lock().unwrap_or_else(|poison| poison.into_inner());
    if slot.is_none() {
        *slot = Some(error);
    }
}

fn session_walk_error(err: ignore::Error) -> ApiError {
    ApiError::new(
        "adapter.session.walk_failed",
        ErrorStage::Discovering,
        err.to_string(),
    )
}

fn should_descend_entry(entry: &DirEntry) -> bool {
    entry.file_name().to_str() != Some(".git")
}

/// Supported session export extensions: `.jsonl` for Claude/Codex, `.json` for Gemini.
fn target_has_supported_session_extension(target: &SessionTarget, path: &Path) -> bool {
    matches!(
        (
            target.provider.as_str(),
            path.extension().and_then(|ext| ext.to_str())
        ),
        ("claude" | "codex", Some("jsonl")) | ("gemini", Some("json"))
    )
}

fn relative_key(root: &Path, file: &Path) -> Result<String> {
    if root.is_file() {
        let name = root.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
            ApiError::new(
                "adapter.session.item_key.invalid",
                ErrorStage::Normalizing,
                "session item key must not be empty",
            )
        })?;
        return Ok(name.to_string());
    }
    let relative = file.strip_prefix(root).unwrap_or(file);
    let key = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    if key.is_empty() {
        return Err(ApiError::new(
            "adapter.session.item_key.invalid",
            ErrorStage::Normalizing,
            "session item key must not be empty",
        ));
    }
    Ok(key)
}

fn safe_item_path(root: &Path, key: &str) -> Result<PathBuf> {
    if root.is_file() {
        return Ok(root.to_path_buf());
    }
    if Path::new(key).is_absolute() || key.split('/').any(|part| part == "..") {
        return Err(ApiError::new(
            "adapter.session.path.escape",
            ErrorStage::Fetching,
            "session item key must stay inside the sessions root",
        )
        .with_context("key", key.to_string()));
    }
    Ok(root.join(key))
}

fn content_fingerprint(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|err| fs_error("read_failed", path, err))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| fs_error("read_failed", path, err))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_prefix(&hasher.finalize(), 16))
}

/// Decode raw file content into a `DecodedSession` for the given target/manifest item.
/// Format is selected by file extension: `.jsonl` decodes via the provider-specific
/// JSONL decoder (Claude vs. Codex have different turn schemas), `.json` decodes via
/// the Gemini single-document decoder.
fn decode_manifest_item(
    target: &SessionTarget,
    item: &ManifestItem,
    text: &str,
) -> Result<DecodedSession> {
    let key = item
        .display_path
        .clone()
        .unwrap_or_else(|| item.source_item_key.0.clone());
    let path = Path::new(&key);
    match (
        target.provider.as_str(),
        path.extension().and_then(|ext| ext.to_str()),
    ) {
        ("codex", Some("jsonl")) => Ok(decode::decode_codex_jsonl(text)),
        ("claude", Some("jsonl")) => Ok(decode::decode_claude_jsonl(text)),
        ("gemini", Some("json")) => decode::decode_gemini_json(text).map_err(|err| {
            ApiError::new(
                "adapter.session.decode_failed",
                ErrorStage::Normalizing,
                err,
            )
            .with_context("path", key.clone())
        }),
        _ => Err(ApiError::new(
            "adapter.session.unsupported_extension",
            ErrorStage::Normalizing,
            "session item has an unsupported file extension",
        )
        .with_context("path", key)),
    }
}

fn item_text(content: ContentRef) -> Result<String> {
    match content {
        ContentRef::InlineText { text } => Ok(text),
        _ => Err(ApiError::new(
            "adapter.session.content_kind.unsupported",
            ErrorStage::Normalizing,
            "session adapter only decodes inline text content",
        )),
    }
}

fn fs_error(code: &str, path: &Path, err: std::io::Error) -> ApiError {
    ApiError::new(
        format!("adapter.session.{code}"),
        ErrorStage::Fetching,
        err.to_string(),
    )
    .with_context("path", path.display().to_string())
}

fn blocking_join_error(err: tokio::task::JoinError) -> ApiError {
    ApiError::new(
        "adapter.session.blocking_task_failed",
        ErrorStage::Planning,
        err.to_string(),
    )
}

fn stage_header(
    job_id: JobId,
    stage_id: &'static str,
    phase: PipelinePhase,
    item_count: usize,
) -> StageResultHeader {
    StageResultHeader {
        job_id,
        stage_id: StageId::new(Uuid::new_v5(&Uuid::NAMESPACE_OID, stage_id.as_bytes())),
        phase,
        status: LifecycleStatus::Completed,
        started_at: timestamp(),
        completed_at: Some(timestamp()),
        counts: StageCounts {
            items_total: Some(item_count as u64),
            items_done: item_count as u64,
            documents_total: Some(item_count as u64),
            documents_done: item_count as u64,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        warnings: Vec::new(),
        error: None,
    }
}

pub(crate) fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}

pub(crate) fn hex_prefix(digest: &[u8], hex_chars: usize) -> String {
    use std::fmt::Write as _;
    let mut token = String::with_capacity(hex_chars);
    for byte in &digest[..(hex_chars / 2).min(digest.len())] {
        let _ = write!(&mut token, "{byte:02x}");
    }
    token
}

fn modified_at(modified: Option<std::time::SystemTime>) -> Option<Timestamp> {
    modified.map(|time| Timestamp(chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339()))
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;

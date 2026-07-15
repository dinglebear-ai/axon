//! AI session transcript source adapter (Claude / Codex / Gemini exports).
//!
//! Like the `git` and `local` adapters, this operates on an already-materialized
//! filesystem tree — a `sessions_root` option pointing at a directory of prepared
//! session export files (or a single file), supplied by the caller. Keeping the
//! live agent scanning out of the adapter makes it unit-testable with fixture
//! files and matches how `git` reads a prepared clone root.
//!
//! Format detection is by file extension: `claude` and `codex` sessions are
//! JSONL (one JSON object per line); `gemini` sessions are a single JSON
//! document. The provider itself comes from the routed `session:<provider>:<id>`
//! target, not from sniffing file content — the router / caller already knows
//! which agent produced the export it is handing to this adapter.

mod decode;
mod metadata;
mod target;

use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use axon_api::source::*;
use ignore::{DirEntry, WalkBuilder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::adapter::{Result, SourceAdapter};
use crate::capability::AdapterCapability;
use crate::manifest::item_identity;

use self::decode::DecodedSession;
pub use self::decode::redact_session_text;
use self::metadata::session_source_document;
pub use self::target::{SessionTarget, parse_session_target};

pub const MODULE_NAME: &str = "sessions";

const ADAPTER_NAME: &str = "session";

#[derive(Debug, Clone, Default)]
pub struct SessionSourceAdapter;

impl SessionSourceAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SourceAdapter for SessionSourceAdapter {
    fn name(&self) -> &'static str {
        ADAPTER_NAME
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    async fn capabilities(&self) -> Result<SourceAdapterCapability> {
        Ok(session_capability(self.version()).into())
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
        let mut documents = Vec::with_capacity(acquisition.fetched_items.len());
        for item in &acquisition.fetched_items {
            let text = item_text(item)?;
            let decoded = decode_item(&target, item, &text)?;
            documents.push(session_source_document(
                plan,
                &target,
                &decoded,
                &acquisition,
                item,
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
        SourceScope::Thread,
    )
    .with_scope(SourceScope::File)
}

fn discover_sync(plan: &SourcePlan) -> Result<SourceManifest> {
    session_capability(env!("CARGO_PKG_VERSION")).validate_scope(plan.route.scope)?;
    validate_adapter(plan)?;
    let target = session_target(plan)?;
    let root = sessions_root(plan)?;

    let mut files = collect_files(&root)?;
    files.sort();

    let base_uri = format!("session://{}/{}", target.provider, target.session_id);
    let mut items = Vec::new();
    for file in files {
        if !has_supported_session_extension(&file) {
            continue;
        }
        let key = relative_key(&root, &file)?;
        let path = safe_item_path(&root, &key)?;
        let meta = fs::metadata(&path).map_err(|err| fs_error("stat_failed", &path, err))?;
        if !meta.is_file() {
            continue;
        }
        let content_hash = content_fingerprint(&path)?;
        let identity = item_identity(SourceKind::Session, &base_uri, &key)?;
        let mut item_metadata = MetadataMap::new();
        item_metadata.insert("session_relative_path".to_string(), json!(key));
        items.push(ManifestItem {
            source_id: plan.route.source.source_id.clone(),
            source_item_key: identity.source_item_key,
            canonical_uri: identity.canonical_uri,
            item_kind: ItemKind::Transcript,
            content_kind: Some(ContentKind::Transcript),
            display_path: Some(key),
            parent_key: None,
            size_bytes: Some(meta.len()),
            content_hash: Some(content_hash),
            mtime: modified_at(meta.modified().ok()),
            version: None,
            fetch_plan: None,
            metadata: item_metadata,
            graph_hints: Vec::new(),
        });
    }
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
                axon_error::ErrorStage::Planning,
                "session adapter requires a sessions_root option pointing at prepared export files",
            )
        })
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
        axon_error::ErrorStage::Routing,
        "route selected a different adapter",
    )
    .with_context("adapter", plan.route.adapter.name.clone()))
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>> {
    if root.is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    let mut builder = WalkBuilder::new(root);
    builder
        .follow_links(false)
        .hidden(false)
        .git_ignore(false)
        .git_exclude(false)
        .parents(false)
        .filter_entry(should_descend_entry);
    let mut files = Vec::new();
    for entry in builder.build() {
        let entry = entry.map_err(|err| {
            ApiError::new(
                "adapter.session.walk_failed",
                axon_error::ErrorStage::Discovering,
                err.to_string(),
            )
        })?;
        if entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            files.push(entry.into_path());
        }
    }
    Ok(files)
}

fn should_descend_entry(entry: &DirEntry) -> bool {
    entry.file_name().to_str() != Some(".git")
}

/// Supported session export extensions: `.jsonl` for Claude/Codex, `.json` for Gemini.
fn has_supported_session_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("jsonl") | Some("json")
    )
}

fn relative_key(root: &Path, file: &Path) -> Result<String> {
    if root.is_file() {
        let name = root.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
            ApiError::new(
                "adapter.session.item_key.invalid",
                axon_error::ErrorStage::Normalizing,
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
            axon_error::ErrorStage::Normalizing,
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
            axon_error::ErrorStage::Fetching,
            "session item key must stay inside the sessions root",
        )
        .with_context("key", key.to_string()));
    }
    Ok(root.join(key))
}

fn content_fingerprint(path: &Path) -> Result<String> {
    let bytes = fs::read(path).map_err(|err| fs_error("read_failed", path, err))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex_prefix(&hasher.finalize(), 16))
}

/// Decode raw file content into a `DecodedSession` for the given target/manifest item.
/// Format is selected by file extension: `.jsonl` decodes via the provider-specific
/// JSONL decoder (Claude vs. Codex have different turn schemas), `.json` decodes via
/// the Gemini single-document decoder.
fn decode_item(
    target: &SessionTarget,
    item: &AcquiredSourceItem,
    text: &str,
) -> Result<DecodedSession> {
    let key = item
        .manifest_item
        .display_path
        .clone()
        .unwrap_or_else(|| item.manifest_item.source_item_key.0.clone());
    let path = Path::new(&key);
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("jsonl") => {
            let decoded = if target.provider.eq_ignore_ascii_case("codex") {
                decode::decode_codex_jsonl(text)
            } else {
                decode::decode_claude_jsonl(text)
            };
            Ok(decoded)
        }
        Some("json") => decode::decode_gemini_json(text).map_err(|err| {
            ApiError::new(
                "adapter.session.decode_failed",
                axon_error::ErrorStage::Normalizing,
                err,
            )
            .with_context("path", key.clone())
        }),
        _ => Err(ApiError::new(
            "adapter.session.unsupported_extension",
            axon_error::ErrorStage::Normalizing,
            "session item has an unsupported file extension",
        )
        .with_context("path", key)),
    }
}

fn item_text(item: &AcquiredSourceItem) -> Result<String> {
    match &item.content_ref {
        ContentRef::InlineText { text } => Ok(text.clone()),
        _ => Err(ApiError::new(
            "adapter.session.content_kind.unsupported",
            axon_error::ErrorStage::Normalizing,
            "session adapter only decodes inline text content",
        )),
    }
}

fn fs_error(code: &str, path: &Path, err: std::io::Error) -> ApiError {
    ApiError::new(
        format!("adapter.session.{code}"),
        axon_error::ErrorStage::Fetching,
        err.to_string(),
    )
    .with_context("path", path.display().to_string())
}

fn blocking_join_error(err: tokio::task::JoinError) -> ApiError {
    ApiError::new(
        "adapter.session.blocking_task_failed",
        axon_error::ErrorStage::Planning,
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

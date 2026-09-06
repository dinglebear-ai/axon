//! Local filesystem source adapter.

mod discovery;
pub(crate) mod local_io;
mod root_state;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use axon_api::source::*;
use base64::Engine as _;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::adapter::{Result, SourceAdapter};
use crate::capability::AdapterCapability;
use crate::local_select::validate_options;

use self::discovery::{
    collect_capped_file_candidates, collect_manifest_items_parallel, hash_file_candidates_parallel,
    manifest_item_from_path, public_base_uri, root_for_item_keys,
};
use self::local_io::{LocalRootHandle, read_content_ref_from_file};
pub use self::root_state::LocalSourceAdapter;

pub const MODULE_NAME: &str = "local";

const ADAPTER_NAME: &str = "local";
const LOCAL_DISCOVERY_HASH_MAX_THREADS: usize = 8;
#[async_trait]
impl SourceAdapter for LocalSourceAdapter {
    fn name(&self) -> &'static str {
        ADAPTER_NAME
    }

    fn version(&self) -> &'static str {
        crate::adapter::SOURCE_ADAPTER_CONTRACT_VERSION
    }

    async fn capabilities(&self) -> Result<SourceAdapterCapability> {
        Ok(local_capability(self.version()).into())
    }

    async fn discover(&self, plan: &SourcePlan) -> Result<SourceManifest> {
        let root_handle = self.root_for_discovery(plan)?;
        let retained_handle = Arc::clone(&root_handle);
        let spool = Arc::new(tempfile::tempdir().map_err(|error| {
            ApiError::new(
                "adapter.local.spool_create_failed",
                ErrorStage::Discovering,
                "failed to create the local discovery content spool",
            )
            .with_context("cause", error.to_string())
        })?);
        let retained_spool = Arc::clone(&spool);
        let job_id = plan.job_id;
        let plan = plan.clone();
        let manifest = tokio::task::spawn_blocking(move || {
            discover_sync(&plan, &root_handle, retained_spool.path())
        })
        .await
        .map_err(blocking_join_error)??;
        self.retain_discovered_root(job_id, retained_handle, spool)?;
        Ok(manifest)
    }

    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &SourceManifestDiff,
    ) -> Result<SourceAcquisition> {
        let root_handle = self.held_root_for_acquisition(plan)?;
        let spool = self.discovery_spool(plan.job_id)?;
        let plan = plan.clone();
        let diff = diff.clone();
        tokio::task::spawn_blocking(move || acquire_sync(&plan, &diff, &root_handle, spool.path()))
            .await
            .map_err(blocking_join_error)?
    }

    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquisition: SourceAcquisition,
    ) -> Result<StageExecutionResult<Vec<SourceDocument>>> {
        let SourceAcquisition {
            source_id,
            fetched_items,
            ..
        } = acquisition;
        let documents = fetched_items
            .into_iter()
            .map(|item| local_source_document(plan, &source_id, item))
            .collect::<Vec<_>>();
        Ok(StageExecutionResult {
            header: stage_header(
                plan.job_id,
                "local_normalize",
                PipelinePhase::Normalizing,
                documents.len(),
            ),
            data: documents,
        })
    }

    fn release(&self, request: &AdapterReleaseRequest) -> Result<()> {
        self.release_root(request.job_id);
        Ok(())
    }
}

fn local_capability(version: &str) -> AdapterCapability {
    AdapterCapability::new(
        AdapterRef {
            name: ADAPTER_NAME.to_string(),
            version: version.to_string(),
        },
        SourceKind::Local,
        SourceScope::File,
    )
    .with_scope(SourceScope::Directory)
    .with_scope(SourceScope::Workspace)
    .with_scope(SourceScope::Repo)
    .with_scope(SourceScope::Map)
}

fn discover_sync(
    plan: &SourcePlan,
    root_handle: &LocalRootHandle,
    spool_dir: &std::path::Path,
) -> Result<SourceManifest> {
    let capability = local_capability(crate::adapter::SOURCE_ADAPTER_CONTRACT_VERSION);
    capability.validate_scope(plan.route.scope)?;
    validate_adapter(plan)?;
    let options = validate_options(&plan.route.validated_options)?;
    if options.follow_symlinks {
        return Err(ApiError::new(
            "adapter.local.symlinks_unsupported",
            ErrorStage::Authorizing,
            "contained local sources do not follow symlinks",
        ));
    }

    let root = PathBuf::from(&plan.request.source);
    let base_uri = public_base_uri(&plan.route.source.canonical_uri);
    let root_for_keys = root_for_item_keys(&root, plan.route.scope);
    let max_items = plan
        .limits
        .effective
        .max_items
        .map(|value| usize::try_from(value).unwrap_or(usize::MAX));

    let mut items = match plan.route.scope {
        SourceScope::File => {
            if max_items == Some(0) {
                Vec::new()
            } else {
                manifest_item_from_path(
                    plan,
                    root_handle,
                    &options,
                    &base_uri,
                    root_for_keys,
                    root.clone(),
                    spool_dir,
                )?
                .into_iter()
                .collect()
            }
        }
        SourceScope::Directory | SourceScope::Workspace | SourceScope::Repo | SourceScope::Map => {
            if let Some(limit) = max_items {
                let candidates = collect_capped_file_candidates(
                    &root,
                    root_for_keys,
                    plan.route.scope,
                    &options,
                    root_handle,
                    limit,
                )?;
                hash_file_candidates_parallel(
                    plan,
                    root_handle,
                    &options,
                    &base_uri,
                    &candidates,
                    spool_dir,
                )?
            } else {
                collect_manifest_items_parallel(
                    plan,
                    root_handle,
                    &options,
                    &base_uri,
                    root_for_keys,
                    &root,
                    spool_dir,
                )?
            }
        }
        _ => {
            return Err(ApiError::new(
                "adapter.local.scope.unsupported",
                ErrorStage::Routing,
                "local adapter only discovers file-like local scopes",
            )
            .with_context("scope", format!("{:?}", plan.route.scope)));
        }
    };
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));

    Ok(SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: SourceGenerationId::from("gen_local_discovery"),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items,
        created_at: timestamp(),
        metadata: MetadataMap::new(),
    })
}

fn acquire_sync(
    plan: &SourcePlan,
    diff: &SourceManifestDiff,
    _root_handle: &LocalRootHandle,
    spool_dir: &std::path::Path,
) -> Result<SourceAcquisition> {
    validate_adapter(plan)?;
    if plan.route.scope == SourceScope::Map {
        return Ok(SourceAcquisition {
            header: stage_header(plan.job_id, "local_fetch", PipelinePhase::Fetching, 0),
            source_id: plan.route.source.source_id.clone(),
            generation: diff.next_generation.clone(),
            adapter: plan.route.adapter.clone(),
            scope: plan.route.scope,
            manifest: SourceManifest {
                source_id: plan.route.source.source_id.clone(),
                generation: diff.next_generation.clone(),
                adapter: plan.route.adapter.clone(),
                scope: plan.route.scope,
                items: diff
                    .added
                    .iter()
                    .chain(diff.modified.iter())
                    .cloned()
                    .collect(),
                created_at: timestamp(),
                metadata: MetadataMap::new(),
            },
            fetched_items: Vec::new(),
            artifacts: Vec::new(),
        });
    }
    let root = PathBuf::from(&plan.request.source);
    let root_for_keys = root_for_item_keys(&root, plan.route.scope);
    let manifest_items = diff
        .added
        .iter()
        .chain(diff.modified.iter())
        .cloned()
        .collect::<Vec<_>>();
    let options = validate_options(&plan.route.validated_options)?;
    let mut fetched_items = Vec::with_capacity(manifest_items.len());
    for item in &manifest_items {
        let path = root_for_keys.join(&item.source_item_key.0);
        if !options.fetches_body(&path) {
            continue;
        }
        let file = std::fs::File::open(discovery::spool_path(spool_dir, &item.source_item_key.0))
            .map_err(|error| {
            local_io::fs_error("adapter.local.spool_read_failed", &path, error)
        })?;
        let acquired_size = file
            .metadata()
            .map_err(|error| local_io::fs_error("adapter.local.stat_failed", &path, error))?
            .len();
        let content_ref = read_content_ref_from_file(file, &path, &options)?;
        let acquired_hash = content_ref_fingerprint(&content_ref)?;
        if item.size_bytes != Some(acquired_size)
            || item.content_hash.as_deref() != Some(&acquired_hash)
        {
            let mut error = ApiError::new(
                "adapter.local.source_changed",
                ErrorStage::Fetching,
                "local source changed between discovery and acquisition; retry the source job",
            )
            .with_context("source_item_key", item.source_item_key.0.clone())
            .with_context(
                "discovered_hash",
                item.content_hash.clone().unwrap_or_default(),
            )
            .with_context("acquired_hash", acquired_hash);
            error.retryable = true;
            return Err(error);
        }
        fetched_items.push(AcquiredSourceItem {
            manifest_item: item.clone(),
            fetch_status: LifecycleStatus::Completed,
            content_ref,
            raw_artifact_id: None,
            headers: RedactedHeaders {
                headers: Vec::new(),
            },
            fetched_at: timestamp(),
            metadata: MetadataMap::new(),
        });
    }

    let manifest = SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: diff.next_generation.clone(),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items: manifest_items,
        created_at: timestamp(),
        metadata: MetadataMap::new(),
    };

    Ok(SourceAcquisition {
        header: stage_header(
            plan.job_id,
            "local_fetch",
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

fn content_ref_fingerprint(content: &ContentRef) -> Result<String> {
    let bytes = match content {
        ContentRef::InlineText { text } => text.as_bytes().to_vec(),
        ContentRef::InlineBytes { bytes_base64, .. } => base64::engine::general_purpose::STANDARD
            .decode(bytes_base64)
            .map_err(|error| {
                ApiError::new(
                    "adapter.local.content_decode_failed",
                    ErrorStage::Fetching,
                    "local binary content could not be verified",
                )
                .with_context("cause", error.to_string())
            })?,
        ContentRef::Artifact { .. } | ContentRef::External { .. } => {
            return Err(ApiError::new(
                "adapter.local.content_verification_unsupported",
                ErrorStage::Fetching,
                "local acquired content is not inline and cannot be verified",
            ));
        }
    };
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn blocking_join_error(err: tokio::task::JoinError) -> ApiError {
    ApiError::new(
        "adapter.local.blocking_task_failed",
        ErrorStage::Planning,
        err.to_string(),
    )
}

fn validate_adapter(plan: &SourcePlan) -> Result<()> {
    if plan.route.adapter.name == ADAPTER_NAME {
        return Ok(());
    }
    Err(ApiError::new(
        "adapter.local.mismatch",
        ErrorStage::Routing,
        "route selected a different adapter",
    )
    .with_context("adapter", plan.route.adapter.name.clone()))
}

fn local_source_document(
    plan: &SourcePlan,
    source_id: &SourceId,
    item: AcquiredSourceItem,
) -> SourceDocument {
    let mut metadata = MetadataMap::new();
    metadata.insert("source_family".to_string(), json!("code"));
    metadata.insert("source_kind".to_string(), json!("local"));
    metadata.insert("source_adapter".to_string(), json!(plan.route.adapter.name));
    metadata.insert("source_scope".to_string(), json!(plan.route.scope));
    metadata.insert(
        "item_canonical_uri".to_string(),
        json!(item.manifest_item.canonical_uri.clone()),
    );
    metadata.insert("committed_generation".to_string(), json!("uncommitted"));
    metadata.insert("visibility".to_string(), json!("internal"));
    metadata.insert("redaction_status".to_string(), json!("clean"));
    SourceDocument {
        document_id: local_document_id(source_id, &item.manifest_item.source_item_key),
        source_id: source_id.clone(),
        source_item_key: item.manifest_item.source_item_key,
        canonical_uri: item.manifest_item.canonical_uri,
        content_kind: item
            .manifest_item
            .content_kind
            .unwrap_or(ContentKind::PlainText),
        content: item.content_ref,
        metadata,
        title: item.manifest_item.display_path.clone(),
        language: None,
        path: item.manifest_item.display_path,
        mime_type: None,
        structured_payload: None,
        artifact_id: item.raw_artifact_id,
        chunk_hints: plan.route.chunking_hints.clone(),
        parser_hints: plan.route.parser_hints.clone(),
    }
}

fn stage_header(
    job_id: JobId,
    stage_id: &'static str,
    phase: PipelinePhase,
    item_count: usize,
) -> StageResultHeader {
    StageResultHeader {
        job_id,
        stage_id: named_stage_id(stage_id),
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

fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}

fn named_stage_id(stage_id: &str) -> StageId {
    StageId::new(Uuid::new_v5(&Uuid::NAMESPACE_OID, stage_id.as_bytes()))
}

fn local_document_id(source_id: &SourceId, item_key: &SourceItemKey) -> DocumentId {
    DocumentId::from(format!(
        "doc_local_{}",
        stable_token(&format!("{}\0{}", source_id.0, item_key.0))
    ))
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

fn modified_at(modified: Option<SystemTime>) -> Option<Timestamp> {
    modified.map(|time| Timestamp(chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339()))
}

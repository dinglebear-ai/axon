//! Durable-memory source adapter.
//!
//! Persistence stays owned by `axon-memory`. This module depends only on the
//! neutral [`MemorySourceProvider`] read boundary, then projects one authorized
//! `MemoryRecord` into the normal source acquisition contract.

use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use axon_api::source::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::adapter::{Result, SourceAdapter};
use crate::capability::AdapterCapability;

const ADAPTER_NAME: &str = "memory";

mod graph;

#[async_trait]
pub trait MemorySourceProvider: Send + Sync {
    async fn get(&self, memory_id: MemoryId) -> Result<Option<MemoryRecord>>;
}

#[derive(Debug, Clone, Copy)]
pub struct MemorySourceAccess {
    pub visibility_ceiling: Visibility,
    pub allow_sensitive: bool,
}

pub struct MemorySourceAdapter {
    provider: Arc<dyn MemorySourceProvider>,
    access: MemorySourceAccess,
    materialized: RwLock<Option<MemoryRecord>>,
}

impl MemorySourceAdapter {
    pub fn new(provider: Arc<dyn MemorySourceProvider>, access: MemorySourceAccess) -> Self {
        Self {
            provider,
            access,
            materialized: RwLock::new(None),
        }
    }

    pub async fn materialize(
        &self,
        plan: SourcePlan,
    ) -> Result<crate::acquisition::MaterializedSource> {
        validate_plan(&plan)?;
        let memory_id = memory_id_from_uri(&plan.route.source.canonical_uri)?;
        let record = self
            .provider
            .get(memory_id.clone())
            .await?
            .ok_or_else(|| missing_memory(&memory_id))?;
        authorize_record(&record, self.access)?;
        *self.materialized.write().map_err(cache_error)? = Some(record);
        Ok(crate::acquisition::MaterializedSource::virtual_source(plan))
    }

    fn record(&self) -> Result<MemoryRecord> {
        self.materialized
            .read()
            .map_err(cache_error)?
            .clone()
            .ok_or_else(|| {
                ApiError::new(
                    "adapter.memory.not_materialized",
                    axon_error::ErrorStage::Planning,
                    "memory source must be materialized before acquisition",
                )
            })
    }
}

#[async_trait]
impl SourceAdapter for MemorySourceAdapter {
    fn name(&self) -> &'static str {
        ADAPTER_NAME
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    async fn capabilities(&self) -> Result<SourceAdapterCapability> {
        Ok(AdapterCapability::new(
            AdapterRef {
                name: ADAPTER_NAME.to_string(),
                version: self.version().to_string(),
            },
            SourceKind::Memory,
            SourceScope::Api,
        )
        .into())
    }

    async fn discover(&self, plan: &SourcePlan) -> Result<SourceManifest> {
        validate_plan(plan)?;
        let record = self.record()?;
        let items = if eligible_for_publication(record.status) {
            vec![manifest_item(plan, &record)?]
        } else {
            Vec::new()
        };
        Ok(SourceManifest {
            source_id: plan.route.source.source_id.clone(),
            generation: SourceGenerationId::new("gen_memory_discovery"),
            adapter: plan.route.adapter.clone(),
            scope: plan.route.scope,
            items,
            created_at: timestamp(),
            metadata: memory_metadata(&record),
        })
    }

    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &SourceManifestDiff,
    ) -> Result<SourceAcquisition> {
        validate_plan(plan)?;
        let record = self.record()?;
        let items = diff
            .added
            .iter()
            .chain(diff.modified.iter())
            .cloned()
            .collect::<Vec<_>>();
        let fetched_items = items
            .iter()
            .cloned()
            .map(|manifest_item| AcquiredSourceItem {
                manifest_item,
                fetch_status: LifecycleStatus::Completed,
                content_ref: ContentRef::InlineText {
                    text: record.body.clone(),
                },
                raw_artifact_id: None,
                headers: RedactedHeaders {
                    headers: Vec::new(),
                },
                fetched_at: timestamp(),
                metadata: memory_metadata(&record),
            })
            .collect::<Vec<_>>();
        let manifest = SourceManifest {
            source_id: plan.route.source.source_id.clone(),
            generation: diff.next_generation.clone(),
            adapter: plan.route.adapter.clone(),
            scope: plan.route.scope,
            items,
            created_at: timestamp(),
            metadata: memory_metadata(&record),
        };
        Ok(SourceAcquisition {
            header: stage_header(
                plan.job_id,
                "memory_acquire",
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

    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquisition: SourceAcquisition,
    ) -> Result<StageExecutionResult<Vec<SourceDocument>>> {
        validate_plan(plan)?;
        let record = self.record()?;
        let documents = acquisition
            .fetched_items
            .iter()
            .map(|item| memory_document(plan, &record, &acquisition, item))
            .collect::<Result<Vec<_>>>()?;
        Ok(StageExecutionResult {
            header: stage_header(
                plan.job_id,
                "memory_normalize",
                PipelinePhase::Normalizing,
                documents.len(),
            ),
            data: documents,
        })
    }
}

pub fn memory_id_from_uri(uri: &str) -> Result<MemoryId> {
    let value = uri
        .strip_prefix("memory://")
        .ok_or_else(|| invalid_uri(uri))?;
    if value.is_empty()
        || value.len() > 200
        || !value.starts_with("mem_")
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_alphanumeric() && !matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(invalid_uri(uri));
    }
    Ok(MemoryId::new(value))
}

fn manifest_item(plan: &SourcePlan, record: &MemoryRecord) -> Result<ManifestItem> {
    let encoded = serde_json::to_vec(record).map_err(|error| {
        ApiError::new(
            "adapter.memory.fingerprint_failed",
            axon_error::ErrorStage::Discovering,
            error.to_string(),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(encoded);
    Ok(ManifestItem {
        source_id: plan.route.source.source_id.clone(),
        source_item_key: SourceItemKey::new(record.memory_id.0.clone()),
        canonical_uri: plan.route.source.canonical_uri.clone(),
        item_kind: ItemKind::MemoryRecord,
        content_kind: Some(ContentKind::PlainText),
        display_path: Some(record.memory_id.0.clone()),
        parent_key: None,
        size_bytes: Some(record.body.len() as u64),
        content_hash: Some(format!("{:x}", hasher.finalize())),
        mtime: record.history.last().map(|event| event.timestamp.clone()),
        version: None,
        fetch_plan: None,
        metadata: memory_metadata(record),
        graph_hints: Vec::new(),
    })
}

fn memory_document(
    plan: &SourcePlan,
    record: &MemoryRecord,
    acquisition: &SourceAcquisition,
    item: &AcquiredSourceItem,
) -> Result<SourceDocument> {
    let mut metadata = memory_metadata(record);
    let candidates = graph::memory_graph_candidates(plan, record, item);
    metadata.insert(
        axon_parse::vertical::VERTICAL_GRAPH_CANDIDATES_METADATA_KEY.to_string(),
        serde_json::to_value(candidates).map_err(|error| {
            ApiError::new(
                "adapter.memory.graph_projection_failed",
                axon_error::ErrorStage::Normalizing,
                error.to_string(),
            )
        })?,
    );
    Ok(SourceDocument {
        document_id: DocumentId::new(record.memory_id.0.clone()),
        source_id: acquisition.source_id.clone(),
        source_item_key: item.manifest_item.source_item_key.clone(),
        canonical_uri: item.manifest_item.canonical_uri.clone(),
        content_kind: ContentKind::PlainText,
        content: item.content_ref.clone(),
        metadata,
        title: record.title.clone(),
        language: None,
        path: None,
        mime_type: Some("text/plain".to_string()),
        structured_payload: None,
        artifact_id: None,
        chunk_hints: plan.route.chunking_hints.clone(),
        parser_hints: plan.route.parser_hints.clone(),
    })
}

fn memory_metadata(record: &MemoryRecord) -> MetadataMap {
    let mut metadata = MetadataMap::new();
    metadata.insert("source_family".to_string(), json!("memory"));
    metadata.insert("source_kind".to_string(), json!("memory"));
    metadata.insert("source_adapter".to_string(), json!(ADAPTER_NAME));
    metadata.insert("source_scope".to_string(), json!("api"));
    metadata.insert("memory_id".to_string(), json!(record.memory_id.0));
    metadata.insert("memory_type".to_string(), json!(record.memory_type));
    metadata.insert("memory_status".to_string(), json!(record.status));
    metadata.insert(
        "memory_recallable".to_string(),
        json!(matches!(
            record.status,
            MemoryStatus::Active
                | MemoryStatus::Review
                | MemoryStatus::Contradicted
                | MemoryStatus::Working
        )),
    );
    metadata.insert("memory_scope_kind".to_string(), json!(record.scope.kind));
    metadata.insert("memory_scope_value".to_string(), json!(record.scope.value));
    metadata.insert("memory_confidence".to_string(), json!(record.confidence));
    metadata.insert("memory_salience".to_string(), json!(record.salience));
    metadata.insert(
        "memory_decay_profile".to_string(),
        json!(memory_decay_profile(record)),
    );
    metadata.insert("memory_link_count".to_string(), json!(record.links.len()));
    metadata.insert(
        "memory_embedding_ref_count".to_string(),
        json!(record.embedding_refs.len()),
    );
    metadata.insert("visibility".to_string(), json!(record.visibility));
    metadata.insert(
        "redaction_version".to_string(),
        json!(axon_core::redact::REDACTION_VERSION),
    );
    metadata
}

fn memory_decay_profile(record: &MemoryRecord) -> String {
    record
        .decay
        .as_ref()
        .map(|policy| policy.profile.clone())
        .unwrap_or_else(|| {
            match record.memory_type.default_decay_profile() {
                DecayProfile::VeryFast => "very_fast",
                DecayProfile::Fast => "fast",
                DecayProfile::Normal => "normal",
                DecayProfile::Slow => "slow",
                DecayProfile::VerySlow => "very_slow",
                DecayProfile::None => "none",
            }
            .to_string()
        })
}

fn authorize_record(record: &MemoryRecord, access: MemorySourceAccess) -> Result<()> {
    let allowed = match record.visibility {
        Visibility::Public | Visibility::Redacted => true,
        Visibility::Internal | Visibility::Derived => {
            access.visibility_ceiling != Visibility::Public
        }
        Visibility::Sensitive => access.allow_sensitive,
    };
    if allowed {
        return Ok(());
    }
    Err(ApiError::new(
        "adapter.memory.visibility_denied",
        axon_error::ErrorStage::Authorizing,
        "caller is not authorized to acquire this memory",
    )
    .with_context("memory_id", record.memory_id.0.clone()))
}

fn eligible_for_publication(status: MemoryStatus) -> bool {
    matches!(
        status,
        MemoryStatus::Active
            | MemoryStatus::Review
            | MemoryStatus::Contradicted
            | MemoryStatus::Working
    )
}

fn validate_plan(plan: &SourcePlan) -> Result<()> {
    if plan.route.adapter.name != ADAPTER_NAME
        || plan.route.source.source_kind != SourceKind::Memory
    {
        return Err(ApiError::new(
            "adapter.memory.mismatch",
            axon_error::ErrorStage::Routing,
            "route selected a different source adapter",
        ));
    }
    memory_id_from_uri(&plan.route.source.canonical_uri).map(|_| ())
}

fn invalid_uri(uri: &str) -> ApiError {
    ApiError::new(
        "adapter.memory.identity_invalid",
        axon_error::ErrorStage::Resolving,
        "memory source must be exactly memory://mem_<id>",
    )
    .with_context("canonical_uri", uri.to_string())
}

fn missing_memory(memory_id: &MemoryId) -> ApiError {
    ApiError::new(
        "adapter.memory.not_found",
        axon_error::ErrorStage::Fetching,
        "memory source identity does not exist",
    )
    .with_context("memory_id", memory_id.0.clone())
}

fn cache_error<T>(_error: std::sync::PoisonError<T>) -> ApiError {
    ApiError::new(
        "adapter.memory.materialization_unavailable",
        axon_error::ErrorStage::Fetching,
        "memory materialization state is unavailable",
    )
}

fn stage_header(
    job_id: JobId,
    stage: &'static str,
    phase: PipelinePhase,
    item_count: usize,
) -> StageResultHeader {
    StageResultHeader {
        job_id,
        stage_id: StageId::new(Uuid::new_v5(&Uuid::NAMESPACE_OID, stage.as_bytes())),
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
    Timestamp::from(chrono::Utc::now())
}

#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;

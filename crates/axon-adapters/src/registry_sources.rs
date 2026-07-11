//! Registry/package source adapter — npm/PyPI/crates.io-style package pages.
//!
//! Pure and unit-testable: reads a prepared registry metadata dump (local
//! JSON, see [`dump::RegistryDump`]) from the `registry_dump_path` adapter
//! option. It never calls a live registry API directly — fetching that dump
//! from npm/PyPI/crates.io is the bridge's job, mirroring how `web`/`local`
//! separate acquisition planning from the pure adapter logic.

pub mod dump;
mod metadata;
mod options;

use async_trait::async_trait;
use axon_api::source::*;
use uuid::Uuid;

use crate::adapter::{Result, SourceAdapter};
use crate::capability::AdapterCapability;
use crate::manifest::item_identity;

use self::dump::{RegistryDump, RegistryDumpVersion};
use self::metadata::{package_markdown, package_metadata, registry_document_id};
use self::options::{RegistryOptions, validate_options};

pub const MODULE_NAME: &str = "registry_sources";

const ADAPTER_NAME: &str = "registry";

#[derive(Debug, Clone, Default)]
pub struct RegistrySourceAdapter;

impl RegistrySourceAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SourceAdapter for RegistrySourceAdapter {
    fn name(&self) -> &'static str {
        ADAPTER_NAME
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    async fn capabilities(&self) -> Result<SourceAdapterCapability> {
        Ok(registry_capability(self.version()).into())
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
        let options = validate_options(&plan.route.validated_options)?;
        let dump = RegistryDump::load(&options.dump_path)?;
        let documents = acquisition
            .fetched_items
            .iter()
            .map(|item| registry_source_document(plan, &acquisition, &dump, item))
            .collect::<Result<Vec<_>>>()?;
        Ok(StageExecutionResult {
            header: stage_header(
                plan.job_id,
                "registry_normalize",
                PipelinePhase::Normalizing,
                documents.len(),
            ),
            data: documents,
        })
    }
}

fn registry_capability(version: &str) -> AdapterCapability {
    AdapterCapability::new(
        AdapterRef {
            name: ADAPTER_NAME.to_string(),
            version: version.to_string(),
        },
        SourceKind::Registry,
        SourceScope::Package,
    )
    .with_scope(SourceScope::Version)
}

fn discover_sync(plan: &SourcePlan) -> Result<SourceManifest> {
    registry_capability(env!("CARGO_PKG_VERSION")).validate_scope(plan.route.scope)?;
    validate_adapter(plan)?;
    let options = validate_options(&plan.route.validated_options)?;
    let dump = RegistryDump::load(&options.dump_path)?;

    let versions = selected_versions(&dump, &options)?;
    let base_uri = public_base_uri(&plan.route.source.canonical_uri, &dump);
    let mut items = Vec::with_capacity(versions.len());
    for version in versions {
        let raw_key = format!("versions/{}", version.version);
        let identity = item_identity(SourceKind::Registry, &base_uri, &raw_key)?;
        let size_bytes = version
            .readme
            .as_ref()
            .map(|readme| readme.len() as u64)
            .unwrap_or(0);
        items.push(ManifestItem {
            source_id: plan.route.source.source_id.clone(),
            source_item_key: identity.source_item_key,
            canonical_uri: identity.canonical_uri,
            item_kind: ItemKind::PackageVersion,
            content_kind: Some(ContentKind::Markdown),
            display_path: Some(format!("{}@{}", dump.package, version.version)),
            parent_key: None,
            size_bytes: Some(size_bytes),
            content_hash: None,
            mtime: version
                .published_at
                .clone()
                .map(axon_api::source::Timestamp),
            version: Some(version.version.clone()),
            fetch_plan: None,
            metadata: MetadataMap::new(),
            graph_hints: Vec::new(),
        });
    }
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));

    Ok(SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: SourceGenerationId::from("gen_registry_discovery"),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items,
        created_at: timestamp(),
        metadata: MetadataMap::new(),
    })
}

fn acquire_sync(plan: &SourcePlan, diff: &SourceManifestDiff) -> Result<SourceAcquisition> {
    validate_adapter(plan)?;
    let options = validate_options(&plan.route.validated_options)?;
    let dump = RegistryDump::load(&options.dump_path)?;

    let manifest_items = diff
        .added
        .iter()
        .chain(diff.modified.iter())
        .cloned()
        .collect::<Vec<_>>();
    let mut fetched_items = Vec::with_capacity(manifest_items.len());
    for item in &manifest_items {
        let version = item.version.as_deref().ok_or_else(|| {
            ApiError::new(
                "adapter.registry.item_version.missing",
                axon_error::ErrorStage::Fetching,
                "registry manifest item is missing its version",
            )
        })?;
        let dump_version = dump.version(version).ok_or_else(|| {
            ApiError::new(
                "adapter.registry.version.not_found",
                axon_error::ErrorStage::Fetching,
                "registry dump does not contain the requested version",
            )
            .with_context("version", version.to_string())
        })?;
        let text = package_markdown(&dump, dump_version);
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
            "registry_fetch",
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

fn selected_versions<'a>(
    dump: &'a RegistryDump,
    options: &RegistryOptions,
) -> Result<Vec<&'a RegistryDumpVersion>> {
    if options.include_all_versions {
        return Ok(dump.versions.iter().collect());
    }
    let latest = dump.latest_version().ok_or_else(|| {
        ApiError::new(
            "adapter.registry.dump_invalid",
            axon_error::ErrorStage::Discovering,
            "registry dump has no versions to select",
        )
    })?;
    Ok(vec![latest])
}

fn registry_source_document(
    plan: &SourcePlan,
    acquisition: &SourceAcquisition,
    dump: &RegistryDump,
    item: &AcquiredSourceItem,
) -> Result<SourceDocument> {
    let version = item.manifest_item.version.as_deref().ok_or_else(|| {
        ApiError::new(
            "adapter.registry.item_version.missing",
            axon_error::ErrorStage::Normalizing,
            "registry manifest item is missing its version",
        )
    })?;
    let dump_version = dump.version(version).ok_or_else(|| {
        ApiError::new(
            "adapter.registry.version.not_found",
            axon_error::ErrorStage::Normalizing,
            "registry dump does not contain the requested version",
        )
        .with_context("version", version.to_string())
    })?;
    let metadata = package_metadata(plan, dump, dump_version);
    Ok(SourceDocument {
        document_id: registry_document_id(
            &acquisition.source_id,
            &item.manifest_item.source_item_key,
        ),
        source_id: acquisition.source_id.clone(),
        source_item_key: item.manifest_item.source_item_key.clone(),
        canonical_uri: item.manifest_item.canonical_uri.clone(),
        content_kind: item
            .manifest_item
            .content_kind
            .unwrap_or(ContentKind::Markdown),
        content: item.content_ref.clone(),
        metadata,
        title: Some(format!("{}@{}", dump.package, dump_version.version)),
        language: None,
        path: item.manifest_item.display_path.clone(),
        mime_type: Some("text/markdown".to_string()),
        structured_payload: None,
        artifact_id: item.raw_artifact_id.clone(),
        chunk_hints: plan.route.chunking_hints.clone(),
        parser_hints: plan.route.parser_hints.clone(),
    })
}

fn validate_adapter(plan: &SourcePlan) -> Result<()> {
    if plan.route.adapter.name == ADAPTER_NAME {
        return Ok(());
    }
    Err(ApiError::new(
        "adapter.registry.mismatch",
        axon_error::ErrorStage::Routing,
        "route selected a different adapter",
    )
    .with_context("adapter", plan.route.adapter.name.clone()))
}

fn public_base_uri(canonical_uri: &str, dump: &RegistryDump) -> String {
    if let Some((scheme, rest)) = canonical_uri.split_once("://")
        && (scheme == "pkg" || scheme == "registry")
    {
        return format!("{scheme}://{}", rest.trim_matches('/'));
    }
    format!("pkg://{}/{}", dump.registry, dump.package)
}

fn blocking_join_error(err: tokio::task::JoinError) -> ApiError {
    ApiError::new(
        "adapter.registry.blocking_task_failed",
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

fn named_stage_id(stage_id: &str) -> StageId {
    StageId::new(Uuid::new_v5(&Uuid::NAMESPACE_OID, stage_id.as_bytes()))
}

fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}

#[cfg(test)]
#[path = "registry_sources_tests.rs"]
mod tests;

//! Web page/site/docs source adapter.
//!
//! Real acquisition (#298 Wave 1b): `discover` enumerates URLs itself (a
//! trivial single item for `Page`, caller-supplied or adapter-discovered URLs
//! for `Map`, or adapter-discovered URL candidates for `Site`/`Docs`) and
//! `acquire` fetches/renders each
//! changed item through the injected [`FetchProvider`]/[`RenderProvider`]
//! boundary — no
//! `manifest.jsonl`/`markdown_root` disk handoff from `axon-services` remains
//! on this path.

mod acquire;
mod manifest_items;
mod metadata;
mod options;
mod site_discovery;
mod url_parts;
mod vertical;
mod warc;

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use uuid::Uuid;

use crate::adapter::{Result, SourceAdapter};
use crate::boundary::{FetchProvider, RenderProvider};
use crate::capability::AdapterCapability;

use self::manifest_items::{map_urls_manifest_items, page_manifest_item};
use self::metadata::{manifest_metadata, web_source_document};

pub use self::warc::{WarcArchive, build_archive as build_warc_archive};

pub const MODULE_NAME: &str = "web";

const ADAPTER_NAME: &str = "web";

#[derive(Clone)]
pub struct WebSourceAdapter {
    fetch: Arc<dyn FetchProvider>,
    render: Arc<dyn RenderProvider>,
}

impl WebSourceAdapter {
    pub fn new(fetch: Arc<dyn FetchProvider>, render: Arc<dyn RenderProvider>) -> Self {
        Self { fetch, render }
    }
}

#[async_trait]
impl SourceAdapter for WebSourceAdapter {
    fn name(&self) -> &'static str {
        ADAPTER_NAME
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    async fn capabilities(&self) -> Result<SourceAdapterCapability> {
        Ok(web_capability(self.version()).into())
    }

    async fn discover(&self, plan: &SourcePlan) -> Result<SourceManifest> {
        web_capability(self.version()).validate_scope(plan.route.scope)?;
        validate_adapter(plan)?;
        let (items, discovery_metadata) = match plan.route.scope {
            SourceScope::Map => {
                if plan.route.validated_options.values.contains_key("map_urls") {
                    (map_urls_manifest_items(plan)?, MetadataMap::new())
                } else {
                    let discovery = site_discovery::manifest_items(plan, false).await?;
                    (discovery.items, discovery.metadata)
                }
            }
            SourceScope::Page => (
                vec![page_manifest_item(plan, self.fetch.as_ref()).await?],
                MetadataMap::new(),
            ),
            _ => {
                let discovery = site_discovery::manifest_items(plan, true).await?;
                (discovery.items, discovery.metadata)
            }
        };
        let mut metadata = manifest_metadata(plan);
        metadata.0.extend(discovery_metadata.0);
        Ok(SourceManifest {
            source_id: plan.route.source.source_id.clone(),
            generation: SourceGenerationId::from("gen_web_discovery"),
            adapter: plan.route.adapter.clone(),
            scope: plan.route.scope,
            items,
            created_at: timestamp(),
            metadata,
        })
    }

    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &SourceManifestDiff,
    ) -> Result<SourceAcquisition> {
        validate_adapter(plan)?;
        if plan.route.scope == SourceScope::Map {
            return Ok(SourceAcquisition {
                header: stage_header(plan.job_id, "web_fetch", PipelinePhase::Fetching, 0),
                source_id: plan.route.source.source_id.clone(),
                generation: diff.next_generation.clone(),
                adapter: plan.route.adapter.clone(),
                scope: plan.route.scope,
                manifest: diff_manifest(plan, diff, Vec::new()),
                fetched_items: Vec::new(),
                artifacts: Vec::new(),
            });
        }

        let manifest_items: Vec<ManifestItem> = diff
            .added
            .iter()
            .chain(diff.modified.iter())
            .cloned()
            .collect();
        let outcome = acquire::acquire_changed_items(
            plan,
            &manifest_items,
            self.fetch.as_ref(),
            self.render.as_ref(),
        )
        .await?;

        let mut header = stage_header(
            plan.job_id,
            "web_fetch",
            PipelinePhase::Fetching,
            outcome.items.len(),
        );
        header.warnings = outcome.warnings;

        Ok(SourceAcquisition {
            header,
            source_id: plan.route.source.source_id.clone(),
            generation: diff.next_generation.clone(),
            adapter: plan.route.adapter.clone(),
            scope: plan.route.scope,
            manifest: diff_manifest(plan, diff, manifest_items),
            fetched_items: outcome.items,
            artifacts: Vec::new(),
        })
    }

    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquisition: SourceAcquisition,
    ) -> Result<StageExecutionResult<Vec<SourceDocument>>> {
        validate_adapter(plan)?;
        let documents = acquisition
            .fetched_items
            .iter()
            .map(|item| web_source_document(plan, &acquisition, item))
            .collect::<Vec<_>>();
        Ok(StageExecutionResult {
            header: stage_header(
                plan.job_id,
                "web_normalize",
                PipelinePhase::Normalizing,
                documents.len(),
            ),
            data: documents,
        })
    }
}

fn web_capability(version: &str) -> AdapterCapability {
    AdapterCapability::new(
        AdapterRef {
            name: ADAPTER_NAME.to_string(),
            version: version.to_string(),
        },
        SourceKind::Web,
        SourceScope::Site,
    )
    .with_scope(SourceScope::Page)
    .with_scope(SourceScope::Docs)
    .with_scope(SourceScope::Map)
}

fn diff_manifest(
    plan: &SourcePlan,
    diff: &SourceManifestDiff,
    items: Vec<ManifestItem>,
) -> SourceManifest {
    SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: diff.next_generation.clone(),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items,
        created_at: timestamp(),
        metadata: manifest_metadata(plan),
    }
}

fn validate_adapter(plan: &SourcePlan) -> Result<()> {
    if plan.route.adapter.name == ADAPTER_NAME {
        return Ok(());
    }
    Err(ApiError::new(
        "adapter.web.mismatch",
        axon_error::ErrorStage::Routing,
        "route selected a different adapter",
    )
    .with_context("adapter", plan.route.adapter.name.clone()))
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

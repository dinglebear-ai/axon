//! Adapter fakes used by contract tests.

use async_trait::async_trait;
use axon_api::source::*;
use uuid::Uuid;

use crate::adapter::{Result, SourceAdapter};
use crate::capability::AdapterCapability;
use crate::manifest::item_identity;

#[derive(Debug, Clone)]
pub struct FakeSourceAdapter {
    capability: AdapterCapability,
    items: Vec<FakeSourceItem>,
}

#[derive(Debug, Clone)]
struct FakeSourceItem {
    key: SourceItemKey,
    content_kind: ContentKind,
    content: String,
}

impl FakeSourceAdapter {
    pub fn new(adapter: AdapterRef) -> Self {
        let (source_kind, default_scope) = source_defaults(&adapter.name);
        Self {
            capability: AdapterCapability::new(adapter, source_kind, default_scope),
            items: Vec::new(),
        }
    }

    pub fn with_scope(mut self, scope: SourceScope) -> Self {
        self.capability = self.capability.with_scope(scope);
        self
    }

    pub fn with_item(
        mut self,
        key: impl Into<String>,
        content_kind: ContentKind,
        content: impl Into<String>,
    ) -> Self {
        self.items.push(FakeSourceItem {
            key: SourceItemKey::from(key.into()),
            content_kind,
            content: content.into(),
        });
        self
    }
}

#[async_trait]
impl SourceAdapter for FakeSourceAdapter {
    fn name(&self) -> &str {
        &self.capability.adapter.name
    }

    fn version(&self) -> &str {
        &self.capability.adapter.version
    }

    async fn capabilities(&self) -> Result<AdapterCapability> {
        Ok(self.capability.clone())
    }

    async fn discover(&self, plan: &SourcePlan) -> Result<SourceManifest> {
        if plan.route.adapter.name != self.capability.adapter.name {
            return Err(ApiError::new(
                "adapter.mismatch",
                axon_error::ErrorStage::Discovering,
                "route selected a different adapter",
            ));
        }
        self.capability.validate_scope(plan.route.scope)?;

        let mut manifest_items = Vec::new();
        for item in &self.items {
            let identity = item_identity(
                plan.route.source.source_kind,
                &plan.route.source.canonical_uri,
                &item.key.0,
            )?;
            let manifest_item = ManifestItem {
                source_id: plan.route.source.source_id.clone(),
                source_item_key: identity.source_item_key,
                canonical_uri: identity.canonical_uri,
                item_kind: item_kind(plan.route.source.source_kind),
                content_kind: Some(item.content_kind),
                display_path: Some(item.key.0.clone()),
                parent_key: None,
                size_bytes: Some(item.content.len() as u64),
                content_hash: None,
                mtime: None,
                version: None,
                fetch_plan: None,
                metadata: MetadataMap::new(),
                graph_hints: Vec::new(),
            };
            manifest_items.push(manifest_item);
        }

        Ok(SourceManifest {
            source_id: plan.route.source.source_id.clone(),
            generation: SourceGenerationId::from("gen_fake"),
            adapter: plan.route.adapter.clone(),
            scope: plan.route.scope,
            items: manifest_items,
            created_at: timestamp(),
            metadata: MetadataMap::new(),
        })
    }

    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &SourceManifestDiff,
    ) -> Result<SourceAcquisition> {
        self.capability.validate_scope(plan.route.scope)?;

        let manifest_items = diff
            .added
            .iter()
            .chain(diff.modified.iter())
            .cloned()
            .collect::<Vec<_>>();
        let mut fetched_items = Vec::new();
        for manifest_item in &manifest_items {
            let content = self
                .items
                .iter()
                .find_map(|item| {
                    let identity = item_identity(
                        plan.route.source.source_kind,
                        &plan.route.source.canonical_uri,
                        &item.key.0,
                    )
                    .ok()?;
                    (identity.source_item_key == manifest_item.source_item_key)
                        .then(|| item.content.clone())
                })
                .unwrap_or_default();
            fetched_items.push(AcquiredSourceItem {
                manifest_item: manifest_item.clone(),
                fetch_status: LifecycleStatus::Completed,
                content_ref: ContentRef::InlineText { text: content },
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
            header: StageResultHeader {
                job_id: plan.job_id,
                stage_id: stage_id(1),
                phase: PipelinePhase::Fetching,
                status: LifecycleStatus::Completed,
                started_at: timestamp(),
                completed_at: Some(timestamp()),
                counts: StageCounts {
                    items_total: Some(fetched_items.len() as u64),
                    items_done: fetched_items.len() as u64,
                    documents_total: Some(fetched_items.len() as u64),
                    documents_done: fetched_items.len() as u64,
                    chunks_total: None,
                    chunks_done: 0,
                    bytes_total: None,
                    bytes_done: 0,
                },
                warnings: Vec::new(),
                error: None,
            },
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
        let documents = acquisition
            .fetched_items
            .iter()
            .map(|item| SourceDocument {
                document_id: DocumentId::from(format!(
                    "doc_{}_{}",
                    acquisition.source_id.0,
                    sanitize_key(&item.manifest_item.source_item_key.0)
                )),
                source_id: acquisition.source_id.clone(),
                source_item_key: item.manifest_item.source_item_key.clone(),
                canonical_uri: item.manifest_item.canonical_uri.clone(),
                content_kind: item
                    .manifest_item
                    .content_kind
                    .unwrap_or(ContentKind::PlainText),
                content: item.content_ref.clone(),
                metadata: MetadataMap::new(),
                title: item.manifest_item.display_path.clone(),
                language: None,
                path: item.manifest_item.display_path.clone(),
                mime_type: None,
                structured_payload: None,
                artifact_id: item.raw_artifact_id.clone(),
                chunk_hints: plan.route.chunking_hints.clone(),
                parser_hints: plan.route.parser_hints.clone(),
            })
            .collect::<Vec<_>>();

        Ok(StageExecutionResult {
            header: StageResultHeader {
                job_id: plan.job_id,
                stage_id: stage_id(2),
                phase: PipelinePhase::Normalizing,
                status: LifecycleStatus::Completed,
                started_at: timestamp(),
                completed_at: Some(timestamp()),
                counts: StageCounts {
                    items_total: Some(documents.len() as u64),
                    items_done: documents.len() as u64,
                    documents_total: Some(documents.len() as u64),
                    documents_done: documents.len() as u64,
                    chunks_total: None,
                    chunks_done: 0,
                    bytes_total: None,
                    bytes_done: 0,
                },
                warnings: Vec::new(),
                error: None,
            },
            data: documents,
        })
    }
}

fn source_defaults(name: &str) -> (SourceKind, SourceScope) {
    match name {
        "web" => (SourceKind::Web, SourceScope::Site),
        "local" => (SourceKind::Local, SourceScope::Directory),
        "github" | "gitlab" | "gitea" | "git" => (SourceKind::Git, SourceScope::Repo),
        "reddit" => (SourceKind::Reddit, SourceScope::Subreddit),
        "youtube" => (SourceKind::Youtube, SourceScope::Video),
        "feed" => (SourceKind::Feed, SourceScope::Feed),
        "cli" => (SourceKind::CliTool, SourceScope::Tool),
        "mcp" => (SourceKind::McpTool, SourceScope::Tool),
        "session" => (SourceKind::Session, SourceScope::Thread),
        "upload" => (SourceKind::Upload, SourceScope::File),
        _ => (SourceKind::Registry, SourceScope::Package),
    }
}

fn item_kind(source_kind: SourceKind) -> ItemKind {
    match source_kind {
        SourceKind::Local => ItemKind::LocalFile,
        SourceKind::Git => ItemKind::RepoFile,
        SourceKind::Web => ItemKind::WebPage,
        SourceKind::Registry => ItemKind::PackageVersion,
        SourceKind::Feed => ItemKind::FeedEntry,
        SourceKind::Youtube => ItemKind::Transcript,
        SourceKind::Session => ItemKind::SessionTurn,
        SourceKind::CliTool => ItemKind::CliOutput,
        SourceKind::McpTool => ItemKind::McpToolOutput,
        SourceKind::Memory => ItemKind::MemoryRecord,
        SourceKind::Upload => ItemKind::Artifact,
        SourceKind::Reddit => ItemKind::WebPage,
    }
}

fn sanitize_key(key: &str) -> String {
    key.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn timestamp() -> Timestamp {
    Timestamp("2026-07-01T00:00:00Z".to_string())
}

fn stage_id(value: u128) -> StageId {
    StageId::new(Uuid::from_u128(value))
}

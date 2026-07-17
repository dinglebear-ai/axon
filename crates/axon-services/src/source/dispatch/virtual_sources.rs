use std::sync::Arc;

use anyhow::Context as _;
use async_trait::async_trait;
use axon_adapters::memory::{MemorySourceAccess, MemorySourceAdapter, MemorySourceProvider};
use axon_adapters::upload::{UploadSourceAdapter, UploadSourceProvider};
use axon_api::source::{
    ArtifactReadResult, AuthMode, AuthScope, AuthSnapshot, MemoryId, MemoryRecord, Visibility,
};
use axon_core::logging::log_info;
use axon_memory::store::MemoryStore;

use super::{dispatch_materialized, family_source_plan};
use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use crate::source::SourceExecutionContext;
use crate::source::result_map::IndexCounts;

struct ServiceMemorySourceProvider {
    store: Arc<dyn MemoryStore>,
}

#[async_trait]
impl MemorySourceProvider for ServiceMemorySourceProvider {
    async fn get(
        &self,
        memory_id: MemoryId,
    ) -> axon_adapters::adapter::Result<Option<MemoryRecord>> {
        self.store.get(memory_id).await
    }
}

struct ServiceUploadSourceProvider {
    ctx: ServiceContext,
}

#[async_trait]
impl UploadSourceProvider for ServiceUploadSourceProvider {
    async fn get(
        &self,
        source_identity: &str,
    ) -> axon_adapters::adapter::Result<Option<ArtifactReadResult>> {
        crate::uploads::resolve_upload_artifact(&self.ctx, source_identity).await
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_memory(
    ctx: &ServiceContext,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    route: &axon_api::source::RoutePlan,
    execution: &SourceExecutionContext,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=memory embed={embed}"
    ));
    let access = MemorySourceAccess {
        visibility_ceiling: auth_snapshot
            .map(|snapshot| snapshot.visibility_ceiling)
            .unwrap_or(Visibility::Internal),
        allow_sensitive: auth_snapshot.is_none_or(|snapshot| {
            matches!(snapshot.auth_mode, AuthMode::TrustedLocal)
                || snapshot.granted_scopes.contains(&AuthScope::Admin)
        }),
    };
    let adapter = Arc::new(MemorySourceAdapter::new(
        Arc::new(ServiceMemorySourceProvider {
            store: crate::memory::memory_store(ctx).await?,
        }),
        access,
    ));
    let materializer = adapter.clone();
    dispatch_materialized(
        runtime,
        adapter.as_ref(),
        family_source_plan(input, route, embed, Some(1), None),
        collection,
        owner_id,
        auth_snapshot,
        execution,
        move |plan| async move {
            materializer
                .materialize(plan)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))
                .context("memory acquisition failed")
        },
    )
    .await
    .context("memory source indexing failed")
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_upload(
    ctx: &ServiceContext,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    route: &axon_api::source::RoutePlan,
    execution: &SourceExecutionContext,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=upload embed={embed}"
    ));
    let adapter = UploadSourceAdapter::new();
    let materializer = adapter.clone();
    let provider = Arc::new(ServiceUploadSourceProvider { ctx: ctx.clone() });
    dispatch_materialized(
        runtime,
        &adapter,
        family_source_plan(input, route, embed, Some(1), None),
        collection,
        owner_id,
        auth_snapshot,
        execution,
        move |plan| async move {
            materializer
                .materialize(plan, provider)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))
                .context("upload materialization failed")
        },
    )
    .await
    .context("upload source indexing failed")
}

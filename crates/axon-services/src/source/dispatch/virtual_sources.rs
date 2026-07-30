use std::sync::Arc;

use anyhow::Context as _;
use axon_adapters::SourceAdapter;
use axon_adapters::memory::MemorySourceAccess;
use axon_api::source::{AuthMode, AuthScope, AuthSnapshot, Visibility};
use axon_core::logging::log_info;

use super::{dispatch_materialized, family_source_plan};
use crate::context::TargetLocalSourceRuntime;
use crate::source::SourceExecutionContext;
use crate::source::result_map::IndexCounts;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_memory(
    adapter: Arc<dyn SourceAdapter>,
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
    let materializer = Arc::clone(&adapter);
    let mut plan = family_source_plan(input, route, embed, Some(1), None);
    access.apply_to_plan(&mut plan);
    dispatch_materialized(
        runtime,
        adapter.as_ref(),
        plan,
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
    adapter: Arc<dyn SourceAdapter>,
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
    let materializer = Arc::clone(&adapter);
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
                .context("upload materialization failed")
        },
    )
    .await
    .context("upload source indexing failed")
}

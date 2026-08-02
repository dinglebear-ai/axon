use std::sync::Arc;

use anyhow::Context as _;
use axon_adapters::SourceAdapter;
use axon_api::source::{AuthSnapshot, SourceScope};
use axon_core::config::Config;
use axon_core::logging::log_info;

use super::super::SourceExecutionContext;
use super::super::result_map::IndexCounts;
use super::web_options::{merge_caller_web_options, web_crawl_options};
use super::{dispatch_materialized, family_source_plan};
use crate::context::TargetLocalSourceRuntime;

/// Web source: adapter-owned discovery/acquisition followed by the same
/// family-blind executor used by every other source. Conditional HTTP reuse is
/// selected through `SourceAdapter::reuse_policy`, not a web-specific runner.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_web(
    adapter: Arc<dyn SourceAdapter>,
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    scope: SourceScope,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    max_pages: Option<u64>,
    max_depth: Option<u32>,
    output: &axon_api::source::OutputPolicy,
    route: &axon_api::source::RoutePlan,
    execution: &SourceExecutionContext,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=web scope={scope:?} embed={embed} max_pages={max_pages:?} max_depth={max_depth:?}"
    ));
    let mut options = web_crawl_options(cfg, max_pages, max_depth);
    merge_caller_web_options(&mut options, &route.validated_options.values, auth_snapshot)?;
    let mut canonical_route = route.clone();
    canonical_route.validated_options = axon_api::source::AdapterOptions { values: options };

    // Map is the declared discovery-only scope. It uses the normal prefix and
    // ledger publication stages while skipping embedding/vector publication.
    let mut plan = family_source_plan(
        input,
        &canonical_route,
        embed && scope != SourceScope::Map,
        max_pages,
        None,
    );
    plan.request.output = output.clone();
    let materializer = Arc::clone(&adapter);
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
                .map_err(anyhow::Error::new)
        },
    )
    .await
    .context("web source indexing failed")
}

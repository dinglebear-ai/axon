//! Local-path source: fs-aware identity resolution, then the shared non-web
//! document pipeline (finding C1 — local no longer runs a private
//! leasing/diffing/generation/vectorize/publish stack; `LocalSourceAdapter`'s
//! `discover`/`acquire`/`normalize` and the generic `non_web` runner do the
//! work every other non-web family already used).
//!
//! Local identity depends on filesystem canonicalization (symlink
//! resolution, absolute-path normalization, file-vs-directory scope) that
//! `axon-route`'s resolver deliberately does not perform — it stays
//! synchronous and IO-free (see `axon-route/src/local_path.rs`'s "Lexical
//! local path normalization" doc comment). This module reproduces that
//! fs-aware resolution — unchanged in shape and hash scheme from the
//! pre-collapse `local_source/local_source_adapter.rs::resolve_adapter_run`
//! — *before* handing off to the shared runner, so existing `src_local_<hash>`
//! source rows keep resolving to the same source id (not `axon-route`'s
//! generic lexical `local://lp_<hash>` scheme, which never touches the
//! filesystem and cannot distinguish `File` vs `Directory` scope). This must
//! happen before `dispatch_materialized`/`index_materialized_source` runs:
//! that shared runner calls `ledger.get_source(source_id)` on entry, ahead of
//! any adapter `materialize()` step, so the plan handed in must already carry
//! the corrected identity.
//!
//! `LocalSourceAdapter` uses the `SourceAdapter::materialize` trait default
//! (a no-op passthrough) — there is nothing left for an adapter-level
//! materialize step to do once this function has already resolved identity
//! and scope.
//!
//! `code_search_refresh.rs`'s code-search auto-refresh caller is NOT routed
//! through this function — it always owns its job outright (no routed
//! `RoutePlan`, no `SourceExecutionContext`), and needs
//! `LocalSourceSelectionPolicy::CodeSearch`-specific behavior (respect
//! `.gitignore`, mark points `visibility: public`) that this unified dispatch
//! path does not carry. It keeps using `crate::local_source::
//! index_local_source_with_job`, which stays in place for that one caller.

use anyhow::Context as _;
use axon_adapters::SourceAdapter as _;
use axon_adapters::local::LocalSourceAdapter;
use axon_api::source::{
    AdapterRef, AuthMode, AuthScope, AuthSnapshot, AuthorityLevel, ConfigSnapshotId,
    EffectiveLimits, MetadataMap, ResolvedSource, RoutePlan, SourceKind, SourceLimits, SourcePlan,
    SourceRequest, SourceScope,
};
use axon_core::config::Config;
use axon_core::logging::log_info;

use super::{dispatch_materialized, placeholder_job_id};
use crate::context::TargetLocalSourceRuntime;
use crate::local_source::local_source_id;
use crate::source::SourceExecutionContext;
use crate::source::authorize::snapshot_allows_scope;
use crate::source::enforce_local_source_policy;
use crate::source::result_map::IndexCounts;

const LOCAL_ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Local-path source: dispatch through the shared non-web document pipeline.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_local(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    route: &RoutePlan,
    execution: &SourceExecutionContext,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=local embed={embed}"
    ));
    let has_local_scope = auth_snapshot
        .map(|snapshot| snapshot_allows_scope(snapshot, AuthScope::Local))
        .unwrap_or(true);
    enforce_local_source_policy(input, has_local_scope)?;

    let plan = local_source_plan(input, route, embed).await?;
    let adapter =
        if auth_snapshot.is_some_and(|snapshot| snapshot.auth_mode == AuthMode::TrustedLocal) {
            LocalSourceAdapter::new()
        } else {
            LocalSourceAdapter::new_contained(
                std::path::Path::new(&plan.request.source),
                plan.route.scope,
                &cfg.source_local_allowed_roots,
            )
            .map_err(anyhow::Error::new)?
        };
    let materializer = adapter.clone();
    dispatch_materialized(
        runtime,
        &adapter,
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
    .context("local source indexing failed")
}

/// Canonicalize `input`, stat file-vs-directory scope, and build a
/// `SourcePlan` whose `route.source` carries local's fs-derived identity.
/// Everything else (`provider_requirements`, `credential_requirements`,
/// `validated_options`, chunking/parser hints, ...) spreads from the already
/// generically-routed `route` parameter — mirrors the pre-collapse
/// `local_source_adapter.rs::routed_plan`'s `if let Some(routed) = &input.route`
/// branch, which is the only branch `dispatch_local` ever exercised (it always
/// passed a real routed plan).
async fn local_source_plan(
    input: &str,
    route: &RoutePlan,
    embed: bool,
) -> anyhow::Result<SourcePlan> {
    let raw_root = std::path::PathBuf::from(input);
    reject_symlinked_source_root(&raw_root).await?;
    let root = tokio::fs::canonicalize(&raw_root)
        .await
        .with_context(|| format!("invalid local source root {}", public_path_hint(&raw_root)))?;
    let root_is_file = tokio::fs::metadata(&root)
        .await
        .with_context(|| {
            format!(
                "failed to stat local source root {}",
                public_path_hint(&root)
            )
        })?
        .is_file();
    let scope = if root_is_file {
        SourceScope::File
    } else {
        SourceScope::Directory
    };
    let source_id = local_source_id(&root);
    let token = source_id
        .0
        .strip_prefix("src_local_")
        .unwrap_or(source_id.0.as_str());
    let canonical_uri = format!("local://{token}");
    let adapter_ref = AdapterRef {
        name: "local".to_string(),
        version: LOCAL_ADAPTER_VERSION.to_string(),
    };
    let resolved_source = ResolvedSource {
        source: root.to_string_lossy().to_string(),
        canonical_uri: canonical_uri.clone(),
        source_id: source_id.clone(),
        source_kind: SourceKind::Local,
        adapter: adapter_ref.clone(),
        default_scope: scope,
        available_scopes: vec![scope],
        authority: AuthorityLevel::UserPinned,
        confidence: 1.0,
        reason: "target local source".to_string(),
        graph: Vec::new(),
        warnings: Vec::new(),
        metadata: MetadataMap::new(),
    };
    let routed_route = RoutePlan {
        source: resolved_source,
        adapter: adapter_ref,
        scope,
        ..route.clone()
    };
    let mut request = SourceRequest::local_path(root.to_string_lossy().to_string(), !root_is_file);
    request.embed = embed;
    request.options = routed_route.validated_options.clone();
    Ok(SourcePlan {
        job_id: placeholder_job_id(),
        request,
        route: routed_route,
        stage_plan: Vec::new(),
        limits: EffectiveLimits {
            request: SourceLimits::default(),
            adapter_defaults: SourceLimits::default(),
            config_defaults: SourceLimits::default(),
            effective: SourceLimits::default(),
        },
        config_snapshot_id: ConfigSnapshotId::new("cfg_local_source"),
        provider_reservations: Vec::new(),
    })
}

async fn reject_symlinked_source_root(root: &std::path::Path) -> anyhow::Result<()> {
    let metadata = tokio::fs::symlink_metadata(root)
        .await
        .with_context(|| format!("invalid local source root {}", public_path_hint(root)))?;
    if metadata.file_type().is_symlink() {
        anyhow::bail!(
            "unsafe local source root {}: symlinks are not allowed",
            public_path_hint(root)
        );
    }
    Ok(())
}

fn public_path_hint(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| "local-source".to_string())
}

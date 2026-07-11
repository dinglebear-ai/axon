//! Full-corpus refresh: re-enqueue source jobs for previously indexed origins.
//!
//! Origin *discovery* has two paths, tried in order:
//!
//! - **Ledger-driven** (issue #298 target path, `LedgerStore::list_sources` —
//!   see `docs/pipeline-unification/runtime/ledger-contract.md`'s Public
//!   Boundary and `docs/pipeline-unification/foundation/source-pipeline.md`'s
//!   `refresh existing` crosswalk row): every registered source is enumerated
//!   directly, paginated via `list_sources`, with no Qdrant facet involved.
//!   Each ledger source already carries its own `SourceId`, so every
//!   ledger-discovered origin re-enqueues through the unified source
//!   pipeline — a `SourceRequest` with `refresh = Force` submitted via
//!   [`crate::source::enqueue::enqueue_source`] (`JobKind::Source`, run by
//!   `SourceRunner` -> `index_source_with_auth`). `SourceRequest.refresh`
//!   semantics decide staleness, not a replayed job-table config blob.
//! - **Legacy Qdrant-facet fallback** (kept ONLY for pre-ledger content — do
//!   not extend it for new origins once every origin is ledger-registered):
//!   used when the ledger is unreachable on this runtime, or reachable but
//!   holds *zero* registered sources — meaning this indexed corpus entirely
//!   predates ledger registration. Every chunk records a `seed_url` payload
//!   field (the crawl start URL or ingest target that produced it — see
//!   `vector::ops::tei::pipeline`); this path facets the collection on
//!   `seed_url` (scoped per `source_type`) to discover origins, then looks
//!   each one up in the ledger by its deterministically-derived `SourceId`
//!   (`axon_route::source_id`, the same id `index_source`/`enqueue_source`
//!   would compute for that origin) in case it was registered after the fact.
//!   Origins without a ledger hit re-enqueue directly via
//!   `crawl_start_with_context`/`ingest_start_with_context`, replaying the
//!   **original job's** stored `axon_crawl_jobs`/`axon_ingest_jobs` config
//!   snapshot when one exists. Only content indexed with the `seed_url` field
//!   participates in this fallback — chunks indexed before origin tracking
//!   shipped carry no `seed_url` and are invisible to the facet (re-crawl/
//!   re-ingest them once to populate the marker).
//!
//! Both paths share the same re-enqueue core once an origin's `SourceId` is
//! known: [`enqueue_via_source_pipeline`].

use std::error::Error;

use crate::context::ServiceContext;
use crate::source::enqueue::enqueue_source;
use crate::source::routing::resolve_source_route;
use axon_api::source::{SourceKind, SourceListRequest, SourceRequest, SourceSummary};
use axon_core::config::Config;
use axon_jobs::ingest::RE_INGESTABLE_SOURCE_TYPES;
use axon_vector::ops::qdrant::{env_usize_clamped, qdrant_facet, qdrant_facet_filtered};

/// Page size used when paginating [`axon_ledger::store::LedgerStore::list_sources`]
/// during ledger-driven discovery.
const LEDGER_LIST_PAGE_SIZE: usize = 200;

/// What `refresh` will do with a single indexed origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshAction {
    /// Re-enqueue a crawl job seeded from the origin URL.
    Crawl,
    /// Re-enqueue an ingest job for the origin target.
    Ingest,
    /// Not re-runnable; carries a human-readable reason.
    Skip(&'static str),
}

impl RefreshAction {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Crawl => "crawl",
            Self::Ingest => "ingest",
            Self::Skip(_) => "skip",
        }
    }
}

/// A distinct indexed origin and the action `refresh` would take for it.
#[derive(Debug, Clone)]
pub struct RefreshOrigin {
    pub seed_url: String,
    pub source_type: String,
    pub chunks: usize,
    pub action: RefreshAction,
    /// `Some(source_id)` when this origin's deterministic `SourceId` is
    /// already registered in the ledger — `execute_refresh` re-enqueues these
    /// through the unified source pipeline. `None` means the origin predates
    /// ledger registration (or the ledger is unavailable), so `execute_refresh`
    /// falls back to the legacy job-table replay path.
    pub ledger_source_id: Option<String>,
}

/// Compute the deterministic `SourceId` `index_source`/`enqueue_source` would
/// assign to `seed_url` and check whether it is already registered in the
/// ledger. Returns `None` (not an error) when routing fails, no data-plane
/// ledger is configured on this runtime, or the source truly isn't
/// registered yet — all of those mean "use the legacy fallback path".
async fn ledger_source_id_for(
    service_context: Option<&ServiceContext>,
    seed_url: &str,
) -> Option<String> {
    let ledger = service_context?
        .target_local_source_runtime()?
        .ledger
        .clone();
    let request = SourceRequest::new(seed_url.to_string());
    let routed = resolve_source_route(&request).ok()?;
    let source_id = routed.route.source.source_id;
    match ledger.get_source(source_id.clone()).await {
        Ok(Some(_)) => Some(source_id.0),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(error = %e, seed_url, "refresh: ledger lookup failed; using legacy fallback for this origin");
            None
        }
    }
}

/// The read-only plan produced by [`plan_refresh`].
#[derive(Debug, Clone, Default)]
pub struct RefreshPlan {
    pub origins: Vec<RefreshOrigin>,
}

impl RefreshPlan {
    pub fn crawl_count(&self) -> usize {
        self.count(RefreshAction::Crawl)
    }

    pub fn ingest_count(&self) -> usize {
        self.count(RefreshAction::Ingest)
    }

    pub fn skip_count(&self) -> usize {
        self.origins
            .iter()
            .filter(|o| matches!(o.action, RefreshAction::Skip(_)))
            .count()
    }

    fn count(&self, action: RefreshAction) -> usize {
        self.origins.iter().filter(|o| o.action == action).count()
    }
}

/// Outcome of [`execute_refresh`].
#[derive(Debug, Clone, Default)]
pub struct RefreshOutcome {
    pub crawl_enqueued: usize,
    pub ingest_enqueued: usize,
    pub skipped: usize,
    /// `(origin, error)` pairs for origins that failed to enqueue.
    pub failures: Vec<(String, String)>,
}

/// Classify an origin into a re-runnable action based on its `source_type` and
/// `seed_url` shape.
fn classify_action(source_type: &str, seed_url: &str) -> RefreshAction {
    if RE_INGESTABLE_SOURCE_TYPES.contains(&source_type) {
        RefreshAction::Ingest
    } else if source_type == "sessions" || source_type == "prepared_sessions" {
        RefreshAction::Skip("sessions are not re-runnable from an origin marker")
    } else if seed_url.starts_with("http://") || seed_url.starts_with("https://") {
        RefreshAction::Crawl
    } else {
        RefreshAction::Skip("origin is not an http(s) URL")
    }
}

/// Keep an origin if the user supplied no filter, or the filter matches the
/// origin's `source_type` exactly, or is a case-insensitive substring of the
/// `seed_url` (lets `axon refresh example.com` narrow to one domain).
fn matches_filter(origin: &RefreshOrigin, filter: Option<&str>) -> bool {
    match filter {
        None => true,
        Some(f) => {
            let f = f.trim().to_lowercase();
            f.is_empty()
                || origin.source_type.eq_ignore_ascii_case(&f)
                || origin.seed_url.to_lowercase().contains(&f)
        }
    }
}

/// Human-readable `source_type` label for a ledger-registered `SourceKind`.
/// Used for `RefreshOrigin::source_type` (CLI table display and
/// [`matches_filter`]) — deliberately coarser than the legacy Qdrant
/// `source_type` payload strings (`"github"`, `"reddit"`, …), since the
/// ledger tracks provider family at the `SourceKind` level.
fn source_kind_label(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Web => "web",
        SourceKind::Local => "local",
        SourceKind::Git => "git",
        SourceKind::Registry => "registry",
        SourceKind::Feed => "feed",
        SourceKind::Reddit => "reddit",
        SourceKind::Youtube => "youtube",
        SourceKind::Session => "session",
        SourceKind::CliTool => "cli_tool",
        SourceKind::McpTool => "mcp_tool",
        SourceKind::Memory => "memory",
        SourceKind::Upload => "upload",
    }
}

/// Classify a ledger-registered source into a re-runnable action based on its
/// `SourceKind` — the ledger-driven counterpart to [`classify_action`], which
/// classifies legacy Qdrant-facet origins by `source_type` string instead.
fn classify_action_for_kind(kind: SourceKind) -> RefreshAction {
    match kind {
        SourceKind::Web | SourceKind::Registry => RefreshAction::Crawl,
        SourceKind::Git | SourceKind::Feed | SourceKind::Youtube | SourceKind::Reddit => {
            RefreshAction::Ingest
        }
        SourceKind::Local => {
            RefreshAction::Skip("local sources are not re-crawlable from an origin marker")
        }
        SourceKind::Session => {
            RefreshAction::Skip("sessions are not re-runnable from an origin marker")
        }
        SourceKind::Memory | SourceKind::Upload | SourceKind::CliTool | SourceKind::McpTool => {
            RefreshAction::Skip("source kind is not re-runnable")
        }
    }
}

/// Turn one ledger-registered [`SourceSummary`] directly into a
/// [`RefreshOrigin`] — no Qdrant facet round-trip needed since the ledger
/// already carries everything `execute_refresh` needs: the canonical URI
/// (`seed_url`), a chunk count for display/sort, and the `SourceId` itself
/// (`ledger_source_id` is always `Some` for these origins).
fn refresh_origin_from_source(source: SourceSummary) -> RefreshOrigin {
    RefreshOrigin {
        source_type: source_kind_label(source.source_kind).to_string(),
        chunks: source.counts.chunks_total as usize,
        action: classify_action_for_kind(source.source_kind),
        ledger_source_id: Some(source.source_id.0),
        seed_url: source.canonical_uri,
    }
}

/// Enumerate every source registered in the ledger via `list_sources`,
/// paginating until exhausted or `cap` is reached.
///
/// Returns `None` — "use the legacy Qdrant-facet fallback" — when there is no
/// reachable ledger on this runtime (no `service_context`, no
/// `target_local_source_runtime`) or the `list_sources` call itself fails.
/// Returns `Some(sources)` (which may be empty) when the ledger answered
/// successfully; `plan_refresh` treats an empty result the same as `None`
/// (falls back), per the module docs.
async fn ledger_registered_sources(
    service_context: Option<&ServiceContext>,
    cap: usize,
) -> Option<Vec<SourceSummary>> {
    let ledger = service_context?
        .target_local_source_runtime()?
        .ledger
        .clone();

    let mut sources = Vec::new();
    let mut cursor = None;
    loop {
        let remaining = cap.saturating_sub(sources.len());
        if remaining == 0 {
            break;
        }
        let request = SourceListRequest {
            source_kind: None,
            adapter: None,
            status: None,
            authority: None,
            watch_enabled: None,
            tag: None,
            query: None,
            limit: Some(remaining.min(LEDGER_LIST_PAGE_SIZE) as u32),
            cursor: cursor.take(),
        };
        let page = match ledger.list_sources(request).await {
            Ok(page) => page,
            Err(e) => {
                tracing::warn!(error = %e, "refresh: ledger list_sources failed; using legacy facet fallback");
                return None;
            }
        };
        let got_any = !page.items.is_empty();
        cursor = page.next_cursor;
        sources.extend(page.items);
        if cursor.is_none() || !got_any {
            break;
        }
    }
    Some(sources)
}

/// Legacy Qdrant-facet origin discovery (documented fallback — see module
/// docs): facets the collection on `seed_url` scoped per `source_type`, then
/// opportunistically checks each actionable origin's ledger registration
/// status via [`ledger_source_id_for`]. Only reached from [`plan_refresh`]
/// when ledger-driven discovery found no registered sources.
async fn facet_discovered_origins(
    cfg: &Config,
    cap: usize,
    filter: Option<&str>,
    service_context: Option<&ServiceContext>,
) -> Result<Vec<RefreshOrigin>, Box<dyn Error>> {
    let source_types = qdrant_facet(cfg, "source_type", 256)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("facet source_type: {e}").into() })?;

    let mut origins = Vec::new();
    for (source_type, _) in source_types {
        let st_filter = serde_json::json!({
            "must": [{ "key": "source_type", "match": { "value": source_type } }]
        });
        let seeds = qdrant_facet_filtered(cfg, "seed_url", cap, st_filter)
            .await
            .map_err(|e| -> Box<dyn Error> {
                format!("facet seed_url for source_type={source_type}: {e}").into()
            })?;
        for (seed_url, chunks) in seeds {
            let action = classify_action(&source_type, &seed_url);
            let ledger_source_id = match action {
                RefreshAction::Skip(_) => None,
                _ => ledger_source_id_for(service_context, &seed_url).await,
            };
            let origin = RefreshOrigin {
                seed_url,
                source_type: source_type.clone(),
                chunks,
                action,
                ledger_source_id,
            };
            if matches_filter(&origin, filter) {
                origins.push(origin);
            }
        }
    }
    Ok(origins)
}

/// Largest origins first, then stable by URL for deterministic output.
fn sort_origins(origins: &mut [RefreshOrigin]) {
    origins.sort_by(|a, b| {
        b.chunks
            .cmp(&a.chunks)
            .then_with(|| a.seed_url.cmp(&b.seed_url))
    });
}

/// Build the refresh plan. Read-only — performs no enqueues.
///
/// Discovery tries the ledger first ([`ledger_registered_sources`]); only when
/// that finds no registered sources (unreachable ledger, or a ledger that
/// genuinely has none yet) does it fall back to the legacy Qdrant-facet path
/// ([`facet_discovered_origins`]) — see the module docs for the full
/// crosswalk. `service_context` is also used by the fallback path to
/// opportunistically look up each actionable origin's ledger registration
/// status; pass `None` to always plan the legacy fallback path (e.g. contexts
/// with no data plane).
pub async fn plan_refresh(
    cfg: &Config,
    filter: Option<&str>,
    service_context: Option<&ServiceContext>,
) -> Result<RefreshPlan, Box<dyn Error>> {
    let cap = env_usize_clamped("AXON_REFRESH_FACET_LIMIT", 10_000, 1, 1_000_000);

    if let Some(sources) = ledger_registered_sources(service_context, cap).await
        && !sources.is_empty()
    {
        let mut origins: Vec<RefreshOrigin> = sources
            .into_iter()
            .map(refresh_origin_from_source)
            .filter(|origin| matches_filter(origin, filter))
            .collect();
        sort_origins(&mut origins);
        return Ok(RefreshPlan { origins });
    }

    let mut origins = facet_discovered_origins(cfg, cap, filter, service_context).await?;
    sort_origins(&mut origins);
    Ok(RefreshPlan { origins })
}

/// Re-enqueue jobs for every actionable origin in `plan`. Always enqueue-only
/// (never blocks on `--wait`); failures are collected per-origin so one bad
/// origin does not abort the run.
///
/// Each origin is re-enqueued with the **original job's** stored config
/// snapshot (max_depth, scoping, budgets, headers, …) when one exists in the
/// jobs DB — re-crawling with current process defaults would silently widen or
/// narrow a previously scoped crawl. Collection and service endpoints always
/// follow the current process config: refresh targets the collection it
/// faceted, not whatever the original job wrote to.
pub async fn execute_refresh(
    cfg: &Config,
    service_context: &ServiceContext,
    plan: &RefreshPlan,
) -> Result<RefreshOutcome, Box<dyn Error>> {
    let mut enqueue_cfg = cfg.clone();
    enqueue_cfg.wait = false;

    // Job-history lookups are best-effort: refresh still works (with current
    // defaults) when the jobs DB is unavailable or holds no prior job.
    let pool = match axon_jobs::store::open_sqlite_pool(&cfg.sqlite_path.to_string_lossy()).await {
        Ok(pool) => Some(pool),
        Err(e) => {
            tracing::warn!(error = %e, "refresh: jobs DB unavailable; re-enqueuing with current config defaults");
            None
        }
    };

    let job_store = service_context.job_store();

    let mut outcome = RefreshOutcome::default();
    for origin in &plan.origins {
        if matches!(origin.action, RefreshAction::Skip(_)) {
            outcome.skipped += 1;
            continue;
        }

        // Ledger-driven path (F1-03/C4-05): this origin's deterministic
        // `SourceId` is already registered, so re-enqueue through the unified
        // source pipeline instead of replaying a legacy job-table config
        // snapshot. Falls through to the legacy path below when there is no
        // unified job store on this runtime (e.g. a CLI context without
        // in-process workers).
        if let (Some(_source_id), Some(store)) = (&origin.ledger_source_id, job_store.as_ref()) {
            match enqueue_via_source_pipeline(origin, store.as_ref()).await {
                Ok(()) => match origin.action {
                    RefreshAction::Crawl => outcome.crawl_enqueued += 1,
                    RefreshAction::Ingest => outcome.ingest_enqueued += 1,
                    RefreshAction::Skip(_) => unreachable!("skip handled above"),
                },
                Err(e) => outcome.failures.push((origin.seed_url.clone(), e)),
            }
            continue;
        }

        // Legacy fallback (documented in the module docs): the origin
        // predates ledger registration, or no unified job store is available
        // on this runtime.
        match origin.action {
            RefreshAction::Skip(_) => unreachable!("skip handled above"),
            RefreshAction::Crawl => {
                let snapshot = match &pool {
                    Some(pool) => {
                        axon_jobs::query::latest_crawl_config_json(pool, &origin.seed_url)
                            .await
                            .unwrap_or_default()
                    }
                    None => None,
                };
                let job_cfg = origin_config(&enqueue_cfg, snapshot.as_deref());
                // `refresh` re-enqueues previously indexed origins as a
                // system-triggered maintenance operation — no per-caller auth
                // identity is available here.
                match crate::crawl::crawl_start_with_context(
                    &job_cfg,
                    std::slice::from_ref(&origin.seed_url),
                    service_context,
                    None,
                    None,
                )
                .await
                {
                    Ok(_) => outcome.crawl_enqueued += 1,
                    Err(e) => outcome
                        .failures
                        .push((origin.seed_url.clone(), e.to_string())),
                }
            }
            RefreshAction::Ingest => {
                let snapshot = match &pool {
                    Some(pool) => axon_jobs::query::latest_ingest_config_json(
                        pool,
                        &origin.source_type,
                        &origin.seed_url,
                    )
                    .await
                    .unwrap_or_default(),
                    None => None,
                };
                let job_cfg = origin_config(&enqueue_cfg, snapshot.as_deref());
                match crate::ingest::classify_target(
                    &origin.seed_url,
                    job_cfg.github_include_source,
                ) {
                    Ok(source) => {
                        // `refresh` re-enqueues previously indexed origins as
                        // a system-triggered maintenance operation — no
                        // per-caller auth identity is available here.
                        match crate::ingest::ingest_start_with_context(
                            &job_cfg,
                            source,
                            service_context,
                            None,
                        )
                        .await
                        {
                            Ok(_) => outcome.ingest_enqueued += 1,
                            Err(e) => outcome
                                .failures
                                .push((origin.seed_url.clone(), e.to_string())),
                        }
                    }
                    Err(e) => outcome
                        .failures
                        .push((origin.seed_url.clone(), e.to_string())),
                }
            }
        }
    }
    Ok(outcome)
}

/// Re-enqueue `origin` through the unified source pipeline: a `SourceRequest`
/// with `refresh = Force` (source-pipeline.md's `refresh existing` crosswalk
/// row), submitted via [`enqueue_source`] as a detached `JobKind::Source` row.
/// `SourceRequest.refresh` semantics (not a replayed config snapshot) decide
/// what actually gets re-fetched once `SourceRunner` picks the job up.
async fn enqueue_via_source_pipeline(
    origin: &RefreshOrigin,
    store: &dyn axon_jobs::boundary::JobStore,
) -> Result<(), String> {
    let request = SourceRequest::new(origin.seed_url.clone())
        .with_refresh(axon_api::source::SourceRefreshPolicy::Force);
    match enqueue_source(request, store, None).await {
        Ok(result) if result.job.is_some() => Ok(()),
        Ok(result) => Err(format!(
            "source refresh for {} did not enqueue a job: {:?}",
            origin.seed_url, result.status
        )),
        Err(e) => Err(e.to_string()),
    }
}

/// Rebuild the config for one origin from the original job's stored snapshot.
///
/// Replays the job-shaping fields (depth, page caps, subdomain scoping,
/// headers, budgets, include-source, …) and pins the runtime back to the
/// current process: collection and service endpoints follow today's config,
/// the worker stamps `seed_url` itself, and refresh is always enqueue-only.
/// Falls back to the base config when no snapshot exists or it fails to parse.
fn origin_config(base: &Config, snapshot_json: Option<&str>) -> Config {
    let Some(snapshot_json) = snapshot_json else {
        return base.clone();
    };
    match axon_jobs::config_snapshot::apply_config_snapshot(base, snapshot_json) {
        Ok(mut replayed) => {
            replayed.collection = base.collection.clone();
            replayed.qdrant_url = base.qdrant_url.clone();
            replayed.tei_url = base.tei_url.clone();
            replayed.seed_url = None;
            replayed.wait = false;
            replayed
        }
        Err(e) => {
            tracing::warn!(error = %e, "refresh: stored job config snapshot failed to apply; using current config defaults");
            base.clone()
        }
    }
}

#[cfg(test)]
#[path = "refresh_tests.rs"]
mod tests;

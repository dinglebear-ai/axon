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
//! - **Qdrant payload-scan discovery fallback** (kept ONLY for pre-ledger
//!   content — do not extend it for new origins once every origin is
//!   ledger-registered):
//!   used when the ledger is unreachable on this runtime, or reachable but
//!   holds *zero* registered sources — meaning this indexed corpus entirely
//!   predates ledger registration. Unified chunks record origin identity under
//!   contract fields (`source_kind`, `source_family`, `source_canonical_uri`,
//!   `item_canonical_uri`, and web-specific `web_seed_url`); this path scrolls
//!   payloads and aggregates those fields to discover origins, then looks each
//!   one up in the ledger by its deterministically-derived `SourceId`
//!   (`axon_route::source_id`, the same id `index_source`/`enqueue_source`
//!   would compute for that origin) in case it was registered after the fact.
//!   Origins without a ledger hit return a migration-required failure so
//!   refresh cannot bypass the unified Source pipeline. Legacy payloads with
//!   bare `source_type`/`seed_url` are still recognized for diagnostics, but
//!   the fallback no longer requires those removed fields or payload indexes.
//!
//! Both paths share the same re-enqueue core once an origin's `SourceId` is
//! known: [`enqueue_via_source_pipeline`].

use std::collections::BTreeMap;
use std::error::Error;

use crate::context::ServiceContext;
use crate::source::enqueue::enqueue_source;
use crate::source::routing::resolve_source_route;
use axon_api::source::{SourceKind, SourceListRequest, SourceRequest, SourceSummary};
use axon_core::config::Config;
use axon_core::env::env_usize_clamped;
use axon_jobs::ingest::RE_INGESTABLE_SOURCE_TYPES;
use axon_vectors::qdrant::QdrantVectorStore;

/// Page size used when paginating [`axon_ledger::store::LedgerStore::list_sources`]
/// during ledger-driven discovery.
const LEDGER_LIST_PAGE_SIZE: usize = 200;

/// What `refresh` will do with a single indexed origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshAction {
    /// Re-enqueue a web/source refresh job seeded from the origin URL.
    Crawl,
    /// Re-enqueue an ingest job for the origin target.
    Ingest,
    /// Not re-runnable; carries a human-readable reason.
    Skip(&'static str),
}

impl RefreshAction {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Crawl => "source_refresh",
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
    /// cannot be safely refreshed until it is re-indexed through SourceRequest.
    pub ledger_source_id: Option<String>,
}

/// Compute the deterministic `SourceId` `index_source`/`enqueue_source` would
/// assign to `seed_url` and check whether it is already registered in the
/// ledger. Returns `None` (not an error) when routing fails, no data-plane
/// ledger is configured on this runtime, or the source truly isn't
/// registered yet — all of those mean "report migration required".
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
            tracing::warn!(error = %e, seed_url, "refresh: ledger lookup failed; origin must be re-indexed before refresh");
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

/// Classify an origin into a re-runnable action based on its payload source
/// label and origin shape. Accepts both legacy labels (`github`, `crawl`) and
/// unified payload labels (`web`, `git`, `feed`, `youtube`, ...).
fn classify_action(source_type: &str, seed_url: &str) -> RefreshAction {
    let source_type = source_type.trim().to_ascii_lowercase();
    if RE_INGESTABLE_SOURCE_TYPES.contains(&source_type.as_str())
        || matches!(
            source_type.as_str(),
            "git" | "feed" | "reddit" | "youtube" | "social" | "media"
        )
    {
        RefreshAction::Ingest
    } else if matches!(
        source_type.as_str(),
        "session" | "sessions" | "prepared_sessions"
    ) {
        RefreshAction::Skip("sessions are not re-runnable from an origin marker")
    } else if matches!(source_type.as_str(), "registry" | "package") {
        RefreshAction::Crawl
    } else if source_type == "local" || source_type == "code" {
        RefreshAction::Skip("local/code source needs ledger registration before refresh")
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
/// [`matches_filter`]) — deliberately coarser than the Qdrant
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
/// classifies payload-discovered origins by source label string instead.
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
/// Returns `None` — "use the Qdrant payload discovery fallback" — when there is no
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
                tracing::warn!(error = %e, "refresh: ledger list_sources failed; using Qdrant payload discovery fallback");
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

fn payload_str<'a>(payload: &'a serde_json::Value, field: &str) -> Option<&'a str> {
    payload
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn chunk_locator_canonical_uri(payload: &serde_json::Value) -> Option<&str> {
    payload
        .get("chunk_locator")
        .and_then(serde_json::Value::as_object)
        .and_then(|locator| locator.get("canonical_uri"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn payload_origin(payload: &serde_json::Value) -> Option<(String, String, Option<String>)> {
    let source_type = payload_str(payload, "source_kind")
        .or_else(|| payload_str(payload, "source_type"))
        .or_else(|| payload_str(payload, "source_family"))?;
    let seed_url = payload_str(payload, "web_seed_url")
        .or_else(|| payload_str(payload, "seed_url"))
        .or_else(|| payload_str(payload, "source_canonical_uri"))
        .or_else(|| payload_str(payload, "item_canonical_uri"))
        .or_else(|| payload_str(payload, "url"))
        .or_else(|| chunk_locator_canonical_uri(payload))?;
    let source_id = payload_str(payload, "source_id").map(str::to_string);
    Some((source_type.to_string(), seed_url.to_string(), source_id))
}

/// Qdrant payload-scan origin discovery (documented fallback — see module
/// docs): scrolls contract payload fields and aggregates origins, then
/// opportunistically checks each actionable origin's ledger registration status
/// via [`ledger_source_id_for`]. Only reached from [`plan_refresh`] when
/// ledger-driven discovery found no registered sources.
async fn payload_discovered_origins(
    cfg: &Config,
    cap: usize,
    filter: Option<&str>,
    service_context: Option<&ServiceContext>,
) -> Result<Vec<RefreshOrigin>, Box<dyn Error>> {
    let store = QdrantVectorStore::new(cfg.qdrant_url.clone(), "qdrant".to_string());
    let mut counts: BTreeMap<(String, String), (usize, Option<String>)> = BTreeMap::new();
    store
        .scroll_pages(
            &cfg.collection,
            None,
            serde_json::json!({"include": [
                "source_id",
                "source_kind",
                "source_type",
                "source_family",
                "web_seed_url",
                "seed_url",
                "source_canonical_uri",
                "item_canonical_uri",
                "url",
                "chunk_locator"
            ]}),
            512,
            |page| {
                for point in page {
                    if let Some((source_type, seed_url, source_id)) = payload_origin(&point.payload)
                    {
                        let (chunks, known_source_id) =
                            counts.entry((source_type, seed_url)).or_insert((0, None));
                        *chunks += 1;
                        if known_source_id.is_none() {
                            *known_source_id = source_id;
                        }
                    }
                    if counts.len() >= cap {
                        return false;
                    }
                }
                true
            },
        )
        .await
        .map_err(|e| -> Box<dyn Error> { format!("scroll origins: {e}").into() })?;

    let mut origins = Vec::new();
    for ((source_type, seed_url), (chunks, payload_source_id)) in counts {
        let action = classify_action(&source_type, &seed_url);
        let ledger_source_id = match action {
            RefreshAction::Skip(_) => None,
            _ => match payload_source_id {
                Some(source_id) => Some(source_id),
                None => ledger_source_id_for(service_context, &seed_url).await,
            },
        };
        let origin = RefreshOrigin {
            seed_url,
            source_type,
            chunks,
            action,
            ledger_source_id,
        };
        if matches_filter(&origin, filter) {
            origins.push(origin);
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
/// genuinely has none yet) does it fall back to the Qdrant payload discovery path
/// ([`payload_discovered_origins`]) — see the module docs for the full
/// crosswalk. `service_context` is also used by the fallback path to
/// opportunistically look up each actionable origin's ledger registration
/// status; pass `None` to always plan the Qdrant payload fallback path (e.g. contexts
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

    let mut origins = payload_discovered_origins(cfg, cap, filter, service_context).await?;
    sort_origins(&mut origins);
    Ok(RefreshPlan { origins })
}

/// Re-enqueue jobs for every actionable origin in `plan`. Always enqueue-only
/// (never blocks on `--wait`); failures are collected per-origin so one bad
/// origin does not abort the run.
///
/// Each actionable origin must already be registered in the source ledger and
/// is re-enqueued as a unified Source job. Payload-discovered origins without a
/// ledger row fail closed so refresh cannot create work that bypasses
/// SourceRequest.
pub async fn execute_refresh(
    _cfg: &Config,
    service_context: &ServiceContext,
    plan: &RefreshPlan,
) -> Result<RefreshOutcome, Box<dyn Error>> {
    let job_store = service_context.job_store();

    let mut outcome = RefreshOutcome::default();
    for origin in &plan.origins {
        if matches!(origin.action, RefreshAction::Skip(_)) {
            outcome.skipped += 1;
            continue;
        }

        // Ledger-driven path (F1-03/C4-05): this origin's deterministic
        // `SourceId` is already registered, so re-enqueue through the unified
        // source pipeline.
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

        outcome.failures.push((
            origin.seed_url.clone(),
            refresh_migration_required(origin).to_string(),
        ));
    }
    Ok(outcome)
}

fn refresh_migration_required(origin: &RefreshOrigin) -> &'static str {
    if origin.ledger_source_id.is_some() {
        "source refresh requires a unified job store on this runtime"
    } else {
        "source refresh requires ledger registration; re-index this origin through the unified Source pipeline before running refresh"
    }
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

#[cfg(test)]
#[path = "refresh_tests.rs"]
mod tests;

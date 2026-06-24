//! Full-corpus refresh: re-enqueue crawl / ingest jobs for previously indexed
//! origins.
//!
//! Every chunk records a `seed_url` payload field (the crawl start URL or ingest
//! target that produced it — see `vector::ops::tei::pipeline`). `refresh` facets
//! the collection on `seed_url` (scoped per `source_type`), classifies each
//! distinct origin into a re-runnable action, and re-enqueues the matching job.
//!
//! Only content indexed with the `seed_url` field participates — chunks indexed
//! before origin tracking shipped carry no `seed_url` and are invisible to the
//! facet (re-crawl/re-ingest them once to populate the marker).

use std::error::Error;

use crate::context::ServiceContext;
use axon_core::config::Config;
use axon_jobs::ingest::RE_INGESTABLE_SOURCE_TYPES;
use axon_vector::ops::qdrant::{env_usize_clamped, qdrant_facet, qdrant_facet_filtered};

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

/// Build the refresh plan by faceting the collection on `seed_url` per
/// `source_type`. Read-only — performs no enqueues.
pub async fn plan_refresh(
    cfg: &Config,
    filter: Option<&str>,
) -> Result<RefreshPlan, Box<dyn Error>> {
    let cap = env_usize_clamped("AXON_REFRESH_FACET_LIMIT", 10_000, 1, 1_000_000);

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
            let origin = RefreshOrigin {
                seed_url,
                source_type: source_type.clone(),
                chunks,
                action,
            };
            if matches_filter(&origin, filter) {
                origins.push(origin);
            }
        }
    }

    // Largest origins first, then stable by URL for deterministic output.
    origins.sort_by(|a, b| {
        b.chunks
            .cmp(&a.chunks)
            .then_with(|| a.seed_url.cmp(&b.seed_url))
    });
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

    let mut outcome = RefreshOutcome::default();
    for origin in &plan.origins {
        match origin.action {
            RefreshAction::Skip(_) => outcome.skipped += 1,
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
                match crate::crawl::crawl_start_with_context(
                    &job_cfg,
                    std::slice::from_ref(&origin.seed_url),
                    service_context,
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
                        match crate::ingest::ingest_start_with_context(
                            &job_cfg,
                            source,
                            service_context,
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

use crate::crates::core::config::Config;
use crate::crates::core::health::build_doctor_report;
use crate::crates::jobs::crawl::{CrawlJob, list_jobs};
use crate::crates::jobs::embed::{EmbedJob, list_embed_jobs};
use crate::crates::jobs::extract::{ExtractJob, list_extract_jobs};
use crate::crates::jobs::graph::{GraphJob, list_graph_jobs};
use crate::crates::jobs::ingest::{IngestJob, list_ingest_jobs};
use crate::crates::jobs::refresh::{RefreshJob, list_refresh_jobs};
use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::types::{
    DedupeResult, DetailedDomainFacet, DetailedDomainsResult, DoctorResult, DomainFacet,
    DomainsResult, Pagination, SourcesResult, StatsResult, StatusResult,
};
use crate::crates::vector::ops::qdrant::{
    dedupe_payload, domains_payload, env_usize_clamped, payload_domain, payload_url,
    qdrant_scroll_pages_selective, sources_payload,
};
use crate::crates::vector::ops::stats::stats_payload;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use tokio::sync::mpsc;

const WATCHDOG_RECLAIM_PREFIX: &str = "watchdog reclaimed stale running ";
const DEFAULT_DOMAINS_DETAILED_LIMIT: usize = 10_000_000;

#[derive(Debug, thiserror::Error)]
#[error("payload parse error: {0}")]
pub struct PayloadParseError(String);

pub fn map_sources_payload(
    payload: &serde_json::Value,
) -> Result<SourcesResult, PayloadParseError> {
    let count = payload
        .get("count")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| PayloadParseError("missing count".into()))? as usize;
    let limit = payload
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| PayloadParseError("missing limit".into()))? as usize;
    let offset = payload
        .get("offset")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| PayloadParseError("missing offset".into()))? as usize;
    let urls = payload
        .get("urls")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| PayloadParseError("missing urls".into()))?
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let url = item
                .get("url")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| PayloadParseError(format!("urls[{i}]: missing url")))?
                .to_string();
            let chunks = item
                .get("chunks")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| PayloadParseError(format!("urls[{i}]: missing chunks")))?
                as usize;
            Ok((url, chunks))
        })
        .collect::<Result<Vec<_>, PayloadParseError>>()?;

    Ok(SourcesResult {
        count,
        limit,
        offset,
        urls,
    })
}

pub fn map_domains_payload(
    payload: &serde_json::Value,
) -> Result<DomainsResult, PayloadParseError> {
    let limit = payload
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| PayloadParseError("missing limit".into()))? as usize;
    let offset = payload
        .get("offset")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| PayloadParseError("missing offset".into()))? as usize;

    let domains = payload
        .get("domains")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| PayloadParseError("missing domains".into()))?
        .iter()
        .enumerate()
        .map(|(i, item)| {
            Ok(DomainFacet {
                domain: item
                    .get("domain")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| PayloadParseError(format!("domains[{i}]: missing domain")))?
                    .to_string(),
                vectors: item
                    .get("vectors")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| PayloadParseError(format!("domains[{i}]: missing vectors")))?
                    as usize,
            })
        })
        .collect::<Result<Vec<_>, PayloadParseError>>()?;

    Ok(DomainsResult {
        domains,
        limit,
        offset,
    })
}

pub fn map_stats_payload(payload: serde_json::Value) -> StatsResult {
    StatsResult { payload }
}

pub fn map_doctor_payload(payload: serde_json::Value) -> DoctorResult {
    DoctorResult { payload }
}

#[must_use = "sources returns a Result that should be handled"]
pub async fn sources(
    cfg: &Config,
    pagination: Pagination,
) -> Result<SourcesResult, Box<dyn Error>> {
    let payload = sources_payload(cfg, pagination.limit, pagination.offset)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("sources facet query failed: {e}").into() })?;
    Ok(map_sources_payload(&payload)?)
}

#[must_use = "domains returns a Result that should be handled"]
pub async fn domains(
    cfg: &Config,
    pagination: Pagination,
) -> Result<DomainsResult, Box<dyn Error>> {
    let payload = domains_payload(cfg, pagination.limit, pagination.offset)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("domains facet query failed: {e}").into() })?;
    Ok(map_domains_payload(&payload)?)
}

pub fn summarize_detailed_domains(payloads: &[serde_json::Value]) -> DetailedDomainsResult {
    summarize_detailed_domains_limited(payloads, None)
}

pub fn summarize_detailed_domains_limited(
    payloads: &[serde_json::Value],
    limit: Option<usize>,
) -> DetailedDomainsResult {
    let mut by_domain: HashMap<String, (usize, HashSet<String>)> = HashMap::new();
    for payload in payloads.iter().take(limit.unwrap_or(payloads.len())) {
        let domain = payload_domain(payload);
        let url = payload_url(payload);
        let entry = by_domain.entry(domain).or_insert((0, HashSet::new()));
        entry.0 += 1;
        if !url.is_empty() {
            entry.1.insert(url);
        }
    }

    let mut domains: Vec<DetailedDomainFacet> = by_domain
        .into_iter()
        .map(|(domain, (vectors, urls))| DetailedDomainFacet {
            domain,
            vectors,
            urls: urls.len(),
        })
        .collect();
    domains.sort_by(|a, b| a.domain.cmp(&b.domain));
    DetailedDomainsResult { domains }
}

#[must_use = "detailed_domains returns a Result that should be handled"]
pub async fn detailed_domains(cfg: &Config) -> Result<DetailedDomainsResult, Box<dyn Error>> {
    let limit = env_usize_clamped(
        "AXON_DOMAINS_DETAILED_LIMIT",
        DEFAULT_DOMAINS_DETAILED_LIMIT,
        1,
        10_000_000,
    );
    // Aggregate directly inside the scroll callback to avoid buffering all payloads.
    // Previous implementation cloned every payload into a Vec before summarizing,
    // spiking memory on large collections.
    let mut by_domain: HashMap<String, (usize, HashSet<String>)> = HashMap::new();
    let mut count = 0usize;
    // Selective payload: only fetch domain + url fields. Avoids transferring
    // multi-KB chunk_text per point — the detailed domains scan only aggregates
    // domain membership and URL sets.
    qdrant_scroll_pages_selective(
        cfg,
        serde_json::json!({"include": ["domain", "url"]}),
        |points: &[serde_json::Value]| {
            for point in points {
                if count >= limit {
                    return false;
                }
                if let Some(payload) = point.get("payload") {
                    let domain = payload_domain(payload);
                    let url = payload_url(payload);
                    let entry = by_domain.entry(domain).or_insert((0, HashSet::new()));
                    entry.0 += 1;
                    if !url.is_empty() {
                        entry.1.insert(url);
                    }
                    count += 1;
                }
            }
            count < limit
        },
    )
    .await
    .map_err(|e| -> Box<dyn Error> { format!("detailed domains scroll failed: {e}").into() })?;

    let mut domains: Vec<DetailedDomainFacet> = by_domain
        .into_iter()
        .map(|(domain, (vectors, urls))| DetailedDomainFacet {
            domain,
            vectors,
            urls: urls.len(),
        })
        .collect();
    domains.sort_by(|a, b| a.domain.cmp(&b.domain));
    Ok(DetailedDomainsResult { domains })
}

#[must_use = "stats returns a Result that should be handled"]
pub async fn stats(cfg: &Config) -> Result<StatsResult, Box<dyn Error>> {
    let payload = stats_payload(cfg)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("stats query failed: {e}").into() })?;
    Ok(map_stats_payload(payload))
}

#[must_use = "doctor returns a Result that should be handled"]
pub async fn doctor(cfg: &Config) -> Result<DoctorResult, Box<dyn Error>> {
    let payload = build_doctor_report(cfg)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("doctor health check failed: {e}").into() })?;
    Ok(map_doctor_payload(payload))
}

#[must_use = "full_status returns a Result that should be handled"]
pub async fn full_status(cfg: &Config) -> Result<StatusResult, Box<dyn Error>> {
    let jobs = load_status_jobs(cfg).await?;
    let payload = build_status_payload(
        &jobs.crawl,
        &jobs.extract,
        &jobs.embed,
        &jobs.ingest,
        &jobs.refresh,
        &jobs.graph,
    );
    let text = [
        "Axon Status".to_string(),
        format!("crawl jobs:   {}", jobs.crawl.len()),
        format!("extract jobs: {}", jobs.extract.len()),
        format!("embed jobs:   {}", jobs.embed.len()),
        format!("ingest jobs:  {}", jobs.ingest.len()),
        format!("refresh jobs: {}", jobs.refresh.len()),
        format!("graph jobs:   {}", jobs.graph.len()),
    ]
    .join("\n");
    Ok(StatusResult { payload, text })
}

// ── Status business logic ───────────────────────────────────────────────────

pub(crate) struct StatusJobs {
    pub crawl: Vec<CrawlJob>,
    pub extract: Vec<ExtractJob>,
    pub embed: Vec<EmbedJob>,
    pub ingest: Vec<IngestJob>,
    pub refresh: Vec<RefreshJob>,
    pub graph: Vec<GraphJob>,
}

/// Filter + view-mode in one pass: drop reclaimed/non-reclaimed jobs, then
/// apply the active-only / recent-only view mode.
fn filter_and_view<T>(
    cfg: &Config,
    jobs: Vec<T>,
    status_of: impl Fn(&T) -> &str,
    error_of: impl Fn(&T) -> Option<&str>,
) -> Vec<T> {
    let reclaimed_only = cfg.reclaimed_status_only;
    let active_only = cfg.active_status_only;
    let recent_only = cfg.recent_status_only;
    jobs.into_iter()
        .filter(|job| include_status_job(status_of(job), error_of(job), reclaimed_only))
        .filter(|job| include_status_view(status_of(job), active_only, recent_only))
        .collect()
}

pub(crate) async fn load_status_jobs(cfg: &Config) -> Result<StatusJobs, Box<dyn Error>> {
    let (crawl_raw, extract_raw, embed_raw, ingest_raw, refresh_raw, graph_raw) = tokio::join!(
        async {
            list_jobs(cfg, 20, 0)
                .await
                .map_err(|e| format!("crawl: {e}"))
        },
        async {
            list_extract_jobs(cfg, 20, 0)
                .await
                .map_err(|e| format!("extract: {e}"))
        },
        async {
            list_embed_jobs(cfg, 20, 0)
                .await
                .map_err(|e| format!("embed: {e}"))
        },
        async {
            list_ingest_jobs(cfg, None, 20, 0)
                .await
                .map_err(|e| format!("ingest: {e}"))
        },
        async {
            list_refresh_jobs(cfg, 20, 0)
                .await
                .map_err(|e| format!("refresh: {e}"))
        },
        async {
            list_graph_jobs(cfg, 20, 0)
                .await
                .map_err(|e| format!("graph: {e}"))
        },
    );

    Ok(StatusJobs {
        crawl: filter_and_view(cfg, crawl_raw?, |j| &j.status, |j| j.error_text.as_deref()),
        extract: filter_and_view(
            cfg,
            extract_raw?,
            |j| &j.status,
            |j| j.error_text.as_deref(),
        ),
        embed: filter_and_view(cfg, embed_raw?, |j| &j.status, |j| j.error_text.as_deref()),
        ingest: filter_and_view(cfg, ingest_raw?, |j| &j.status, |j| j.error_text.as_deref()),
        refresh: filter_and_view(
            cfg,
            refresh_raw?,
            |j| &j.status,
            |j| j.error_text.as_deref(),
        ),
        graph: filter_and_view(cfg, graph_raw?, |j| &j.status, |j| j.error_text.as_deref()),
    })
}

pub(crate) fn build_status_payload(
    crawl_jobs: &[CrawlJob],
    extract_jobs: &[ExtractJob],
    embed_jobs: &[EmbedJob],
    ingest_jobs: &[IngestJob],
    refresh_jobs: &[RefreshJob],
    graph_jobs: &[GraphJob],
) -> serde_json::Value {
    serde_json::json!({
        "local_crawl_jobs": crawl_jobs,
        "local_extract_jobs": extract_jobs,
        "local_embed_jobs": embed_jobs,
        "local_ingest_jobs": ingest_jobs,
        "local_refresh_jobs": refresh_jobs,
        "local_graph_jobs": graph_jobs,
    })
}

fn include_status_view(status: &str, active_only: bool, recent_only: bool) -> bool {
    if active_only {
        return matches!(status, "pending" | "running" | "processing" | "scraping");
    }
    if recent_only {
        return matches!(
            status,
            "pending" | "running" | "processing" | "scraping" | "completed"
        );
    }
    true
}

fn include_status_job(status: &str, error_text: Option<&str>, reclaimed_only: bool) -> bool {
    let reclaimed = is_watchdog_reclaimed_failure(status, error_text);
    if reclaimed_only {
        reclaimed
    } else {
        !reclaimed
    }
}

fn is_watchdog_reclaimed_failure(status: &str, error_text: Option<&str>) -> bool {
    if status != "failed" {
        return false;
    }
    error_text
        .map(str::trim_start)
        .is_some_and(|text| text.starts_with(WATCHDOG_RECLAIM_PREFIX))
}

#[must_use = "dedupe returns a Result that should be handled"]
pub async fn dedupe(
    cfg: &Config,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<DedupeResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "starting dedupe".to_string(),
        },
    )
    .await;
    // Run dedupe and immediately convert the Result to a plain String outcome so
    // that `Box<dyn Error>` (!Send) is fully dropped before the next `.await`.
    enum DedupeOutcome {
        Success {
            duplicate_groups: usize,
            deleted: usize,
        },
        Failure(String),
    }
    let outcome = match dedupe_payload(cfg).await {
        Ok(v) => DedupeOutcome::Success {
            duplicate_groups: v
                .get("duplicate_groups")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize,
            deleted: v
                .get("deleted")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize,
        },
        Err(e) => DedupeOutcome::Failure(format!("dedupe failed: {e}")),
    };
    match outcome {
        DedupeOutcome::Success {
            duplicate_groups,
            deleted,
        } => {
            emit(
                &tx,
                ServiceEvent::Log {
                    level: LogLevel::Info,
                    message: format!(
                        "completed dedupe: {duplicate_groups} groups, {deleted} deleted"
                    ),
                },
            )
            .await;
            Ok(DedupeResult {
                completed: true,
                duplicate_groups,
                deleted,
            })
        }
        DedupeOutcome::Failure(msg) => {
            emit(
                &tx,
                ServiceEvent::Log {
                    level: LogLevel::Error,
                    message: msg.clone(),
                },
            )
            .await;
            Err(msg.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── map_sources_payload ───────────────────────────────────────────────────

    #[test]
    fn map_sources_valid() {
        let payload = json!({
            "count": 2,
            "limit": 10,
            "offset": 0,
            "urls": [
                { "url": "https://example.com/a", "chunks": 3 },
                { "url": "https://example.com/b", "chunks": 7 }
            ]
        });
        let result = map_sources_payload(&payload).unwrap();
        assert_eq!(result.count, 2);
        assert_eq!(result.limit, 10);
        assert_eq!(result.offset, 0);
        assert_eq!(result.urls.len(), 2);
        assert_eq!(result.urls[0], ("https://example.com/a".to_string(), 3));
        assert_eq!(result.urls[1], ("https://example.com/b".to_string(), 7));
    }

    #[test]
    fn map_sources_missing_count() {
        let payload = json!({ "limit": 10, "offset": 0, "urls": [] });
        let err = map_sources_payload(&payload).unwrap_err();
        assert!(
            err.to_string().contains("count"),
            "error must mention 'count', got: {err}"
        );
    }

    #[test]
    fn map_sources_missing_urls() {
        let payload = json!({ "count": 0, "limit": 10, "offset": 0 });
        let err = map_sources_payload(&payload).unwrap_err();
        assert!(
            err.to_string().contains("urls"),
            "error must mention 'urls', got: {err}"
        );
    }

    #[test]
    fn map_sources_url_entry_missing_url_field() {
        let payload = json!({
            "count": 1,
            "limit": 10,
            "offset": 0,
            "urls": [{ "chunks": 5 }]
        });
        let err = map_sources_payload(&payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("urls[0]"),
            "error must reference urls[0], got: {msg}"
        );
    }

    #[test]
    fn map_sources_url_entry_missing_chunks_field() {
        let payload = json!({
            "count": 1,
            "limit": 10,
            "offset": 0,
            "urls": [{ "url": "https://example.com" }]
        });
        let err = map_sources_payload(&payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("chunks"),
            "error must mention 'chunks', got: {msg}"
        );
    }

    #[test]
    fn map_sources_empty_urls_array() {
        let payload = json!({ "count": 0, "limit": 50, "offset": 0, "urls": [] });
        let result = map_sources_payload(&payload).unwrap();
        assert_eq!(result.count, 0);
        assert!(result.urls.is_empty());
    }

    // ── map_domains_payload ───────────────────────────────────────────────────

    #[test]
    fn map_domains_valid() {
        let payload = json!({
            "limit": 20,
            "offset": 5,
            "domains": [
                { "domain": "example.com", "vectors": 42 },
                { "domain": "docs.rs", "vectors": 100 }
            ]
        });
        let result = map_domains_payload(&payload).unwrap();
        assert_eq!(result.limit, 20);
        assert_eq!(result.offset, 5);
        assert_eq!(result.domains.len(), 2);
        assert_eq!(result.domains[0].domain, "example.com");
        assert_eq!(result.domains[0].vectors, 42);
        assert_eq!(result.domains[1].domain, "docs.rs");
        assert_eq!(result.domains[1].vectors, 100);
    }

    #[test]
    fn map_domains_missing_domains_field() {
        let payload = json!({ "limit": 10, "offset": 0 });
        let err = map_domains_payload(&payload).unwrap_err();
        assert!(
            err.to_string().contains("domains"),
            "error must mention 'domains', got: {err}"
        );
    }

    #[test]
    fn map_domains_entry_missing_domain_key() {
        let payload = json!({
            "limit": 10,
            "offset": 0,
            "domains": [{ "vectors": 5 }]
        });
        let err = map_domains_payload(&payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("domains[0]"),
            "error must reference domains[0], got: {msg}"
        );
    }

    #[test]
    fn map_domains_empty() {
        let payload = json!({ "limit": 10, "offset": 0, "domains": [] });
        let result = map_domains_payload(&payload).unwrap();
        assert!(result.domains.is_empty());
        assert_eq!(result.limit, 10);
        assert_eq!(result.offset, 0);
    }

    // ── status helpers ──────────────────────────────────────────────────────────

    #[test]
    fn watchdog_reclaim_detection_matches_prefix_on_failed_jobs() {
        assert!(is_watchdog_reclaimed_failure(
            "failed",
            Some("watchdog reclaimed stale running ingest job (idle=360s marker=amqp)")
        ));
        assert!(!is_watchdog_reclaimed_failure(
            "error",
            Some("watchdog reclaimed stale running crawl job (idle=361s marker=polling)")
        ));
        assert!(!is_watchdog_reclaimed_failure(
            "completed",
            Some("watchdog reclaimed stale running ingest job (idle=360s marker=amqp)")
        ));
        assert!(!is_watchdog_reclaimed_failure(
            "failed",
            Some("network timeout")
        ));
    }

    #[test]
    fn status_filter_hides_reclaimed_by_default_and_shows_in_reclaimed_mode() {
        let reclaimed_err =
            Some("watchdog reclaimed stale running extract job (idle=360s marker=amqp)");
        assert!(!include_status_job("failed", reclaimed_err, false));
        assert!(include_status_job("failed", reclaimed_err, true));
        assert!(include_status_job("completed", None, false));
        assert!(!include_status_job("completed", None, true));
    }

    #[test]
    fn status_payload_includes_refresh_jobs_key() {
        let payload = build_status_payload(&[], &[], &[], &[], &[], &[]);
        assert!(payload.get("local_refresh_jobs").is_some());
        assert!(payload.get("local_graph_jobs").is_some());
    }

    #[test]
    fn status_view_mode_filters_active_and_recent() {
        assert!(include_status_view("running", true, false));
        assert!(!include_status_view("failed", true, false));
        assert!(include_status_view("completed", false, true));
        assert!(!include_status_view("canceled", false, true));
    }
}

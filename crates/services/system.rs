use crate::crates::core::config::Config;
use crate::crates::core::health::build_doctor_report;
use crate::crates::jobs::crawl::{CrawlJob, list_jobs};
use crate::crates::jobs::embed::{EmbedJob, list_embed_jobs};
use crate::crates::jobs::extract::{ExtractJob, list_extract_jobs};
use crate::crates::jobs::ingest::{IngestJob, list_ingest_jobs};
use crate::crates::jobs::refresh::{RefreshJob, list_refresh_jobs};
use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::types::{
    DedupeResult, DetailedDomainFacet, DetailedDomainsResult, DoctorResult, DomainFacet,
    DomainsResult, Pagination, SourcesResult, StatsResult, StatusResult,
};
use crate::crates::vector::ops::qdrant::{
    dedupe_payload, domains_payload, payload_domain, payload_url, qdrant_scroll_pages,
    sources_payload,
};
use crate::crates::vector::ops::stats::stats_payload;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use tokio::sync::mpsc;

const WATCHDOG_RECLAIM_PREFIX: &str = "watchdog reclaimed stale running ";

#[derive(Debug)]
pub struct PayloadParseError(String);
impl fmt::Display for PayloadParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "payload parse error: {}", self.0)
    }
}
impl Error for PayloadParseError {}

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

pub async fn sources(
    cfg: &Config,
    pagination: Pagination,
) -> Result<SourcesResult, Box<dyn Error>> {
    let payload = sources_payload(cfg, pagination.limit, pagination.offset).await?;
    Ok(map_sources_payload(&payload)?)
}

pub async fn domains(
    cfg: &Config,
    pagination: Pagination,
) -> Result<DomainsResult, Box<dyn Error>> {
    let payload = domains_payload(cfg, pagination.limit, pagination.offset).await?;
    Ok(map_domains_payload(&payload)?)
}

pub fn summarize_detailed_domains(payloads: &[serde_json::Value]) -> DetailedDomainsResult {
    let mut by_domain: HashMap<String, (usize, HashSet<String>)> = HashMap::new();
    for payload in payloads {
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

pub async fn detailed_domains(cfg: &Config) -> Result<DetailedDomainsResult, Box<dyn Error>> {
    let mut payloads = Vec::new();
    qdrant_scroll_pages(cfg, |points| {
        for point in points {
            if let Some(payload) = point.get("payload") {
                payloads.push(payload.clone());
            }
        }
    })
    .await?;
    Ok(summarize_detailed_domains(&payloads))
}

pub async fn stats(cfg: &Config) -> Result<StatsResult, Box<dyn Error>> {
    let payload = stats_payload(cfg).await?;
    Ok(map_stats_payload(payload))
}

pub async fn doctor(cfg: &Config) -> Result<DoctorResult, Box<dyn Error>> {
    let payload = build_doctor_report(cfg).await?;
    Ok(map_doctor_payload(payload))
}

pub async fn full_status(cfg: &Config) -> Result<StatusResult, Box<dyn Error>> {
    let jobs = load_status_jobs(cfg).await?;
    let payload = build_status_payload(
        &jobs.crawl,
        &jobs.extract,
        &jobs.embed,
        &jobs.ingest,
        &jobs.refresh,
    );
    let text = [
        "Axon Status".to_string(),
        format!("crawl jobs:   {}", jobs.crawl.len()),
        format!("extract jobs: {}", jobs.extract.len()),
        format!("embed jobs:   {}", jobs.embed.len()),
        format!("ingest jobs:  {}", jobs.ingest.len()),
        format!("refresh jobs: {}", jobs.refresh.len()),
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
}

fn filter_status_jobs<T, FStatus, FError>(
    jobs: Vec<T>,
    reclaimed_only: bool,
    status_of: FStatus,
    error_of: FError,
) -> Vec<T>
where
    FStatus: Fn(&T) -> &str,
    FError: Fn(&T) -> Option<&str>,
{
    jobs.into_iter()
        .filter(|job| include_status_job(status_of(job), error_of(job), reclaimed_only))
        .collect()
}

async fn list_crawl_status(cfg: &Config) -> Result<Vec<CrawlJob>, String> {
    list_jobs(cfg, 20, 0)
        .await
        .map_err(|e| format!("crawl status lookup failed: {e}"))
}

async fn list_extract_status(cfg: &Config) -> Result<Vec<ExtractJob>, String> {
    list_extract_jobs(cfg, 20, 0)
        .await
        .map_err(|e| format!("extract status lookup failed: {e}"))
}

async fn list_embed_status(cfg: &Config) -> Result<Vec<EmbedJob>, String> {
    list_embed_jobs(cfg, 20, 0)
        .await
        .map_err(|e| format!("embed status lookup failed: {e}"))
}

async fn list_ingest_status(cfg: &Config) -> Result<Vec<IngestJob>, String> {
    list_ingest_jobs(cfg, 20, 0)
        .await
        .map_err(|e| format!("ingest status lookup failed: {e}"))
}

async fn list_refresh_status(cfg: &Config) -> Result<Vec<RefreshJob>, String> {
    list_refresh_jobs(cfg, 20, 0)
        .await
        .map_err(|e| format!("refresh status lookup failed: {e}"))
}

pub(crate) async fn load_status_jobs(cfg: &Config) -> Result<StatusJobs, Box<dyn Error>> {
    let (crawl_raw, extract_raw, embed_raw, ingest_raw, refresh_raw) = tokio::join!(
        list_crawl_status(cfg),
        list_extract_status(cfg),
        list_embed_status(cfg),
        list_ingest_status(cfg),
        list_refresh_status(cfg),
    );
    let reclaimed_only = cfg.reclaimed_status_only;
    let crawl = filter_status_jobs(
        crawl_raw?,
        reclaimed_only,
        |job| &job.status,
        |job| job.error_text.as_deref(),
    );
    let extract = filter_status_jobs(
        extract_raw?,
        reclaimed_only,
        |job| &job.status,
        |job| job.error_text.as_deref(),
    );
    let embed = filter_status_jobs(
        embed_raw?,
        reclaimed_only,
        |job| &job.status,
        |job| job.error_text.as_deref(),
    );
    let ingest = filter_status_jobs(
        ingest_raw?,
        reclaimed_only,
        |job| &job.status,
        |job| job.error_text.as_deref(),
    );
    let refresh = filter_status_jobs(
        refresh_raw?,
        reclaimed_only,
        |job| &job.status,
        |job| job.error_text.as_deref(),
    );
    Ok(StatusJobs {
        crawl,
        extract,
        embed,
        ingest,
        refresh,
    })
}

pub(crate) fn build_status_payload(
    crawl_jobs: &[CrawlJob],
    extract_jobs: &[ExtractJob],
    embed_jobs: &[EmbedJob],
    ingest_jobs: &[IngestJob],
    refresh_jobs: &[RefreshJob],
) -> serde_json::Value {
    serde_json::json!({
        "local_crawl_jobs": crawl_jobs,
        "local_extract_jobs": extract_jobs,
        "local_embed_jobs": embed_jobs,
        "local_ingest_jobs": ingest_jobs,
        "local_refresh_jobs": refresh_jobs,
    })
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
    );
    let payload = match dedupe_payload(cfg).await {
        Ok(v) => v,
        Err(e) => {
            emit(
                &tx,
                ServiceEvent::Log {
                    level: LogLevel::Error,
                    message: format!("dedupe failed: {e}"),
                },
            );
            return Err(e);
        }
    };
    let duplicate_groups = payload
        .get("duplicate_groups")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    let deleted = payload
        .get("deleted")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("completed dedupe: {duplicate_groups} groups, {deleted} deleted"),
        },
    );
    Ok(DedupeResult {
        completed: true,
        duplicate_groups,
        deleted,
    })
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
        let payload = build_status_payload(&[], &[], &[], &[], &[]);
        assert!(payload.get("local_refresh_jobs").is_some());
    }
}

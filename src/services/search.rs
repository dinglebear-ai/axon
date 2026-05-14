mod synthesis;

pub use synthesis::research;
pub use synthesis::research_payload;

use crate::core::config::Config;
use crate::services::events::{LogLevel, ServiceEvent, emit};
use crate::services::types::{ResearchResult, SearchOptions, SearchResult, ServiceTimeRange};
use spider_agent::{Agent, SearchOptions as SpiderSearchOptions, TimeRange};
use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};
use tokio::sync::mpsc;

const REDACTED_TOKEN: &str = "[redacted-token]";

pub(super) fn to_spider_time_range(tr: ServiceTimeRange) -> TimeRange {
    match tr {
        ServiceTimeRange::Day => TimeRange::Day,
        ServiceTimeRange::Week => TimeRange::Week,
        ServiceTimeRange::Month => TimeRange::Month,
        ServiceTimeRange::Year => TimeRange::Year,
    }
}

pub(super) fn query_log_summary(query: &str, cfg: &Config) -> String {
    let mut hasher = DefaultHasher::new();
    query.hash(&mut hasher);
    let hash = hasher.finish();
    let preview = if log_full_queries(cfg) {
        query.to_string()
    } else {
        redact_token_like_substrings(query)
            .chars()
            .take(48)
            .collect::<String>()
    };
    format!(
        "len={} hash={hash:016x} preview={preview:?}",
        query.chars().count()
    )
}

fn log_full_queries(cfg: &Config) -> bool {
    std::env::var("AXON_LOG_FULL_QUERIES")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
        || cfg
            .log_level
            .as_deref()
            .map(|level| {
                matches!(
                    level.trim().to_ascii_lowercase().as_str(),
                    "debug" | "trace"
                )
            })
            .unwrap_or(false)
}

fn redact_token_like_substrings(input: &str) -> String {
    input
        .split_whitespace()
        .map(|token| {
            let trimmed = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
            if looks_like_secret_token(trimmed) {
                token.replace(trimmed, REDACTED_TOKEN)
            } else {
                token.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn looks_like_secret_token(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    lower.starts_with("sk-")
        || lower.starts_with("ghp_")
        || lower.starts_with("github_pat_")
        || lower.starts_with("atk_")
        || (token.len() >= 20
            && token.chars().any(|c| c.is_ascii_alphabetic())
            && token.chars().any(|c| c.is_ascii_digit()))
}

/// Execute a Tavily web search and return raw JSON result items.
pub async fn search_results(
    cfg: &Config,
    query: &str,
    limit: usize,
    offset: usize,
    time_range: Option<TimeRange>,
) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    if cfg.tavily_api_key.is_empty() {
        return Err("search requires TAVILY_API_KEY — set it in .env".into());
    }
    let mut search_opts = SpiderSearchOptions::new().with_limit((limit + offset).clamp(1, 100));
    if let Some(tr) = time_range {
        search_opts = search_opts.with_time_range(tr);
    }
    let agent = Agent::builder()
        .with_search_tavily(&cfg.tavily_api_key)
        .build()?;
    let results = agent.search_with_options(query, search_opts).await?;
    Ok(results
        .results
        .iter()
        .skip(offset)
        .take(limit)
        .map(|r| {
            serde_json::json!({
                "position": r.position,
                "title": r.title,
                "url": r.url,
                "snippet": r.snippet,
            })
        })
        .collect())
}

/// Map a `Vec<serde_json::Value>` of raw search items into a typed [`SearchResult`].
pub fn map_search_results(results: Vec<serde_json::Value>) -> SearchResult {
    SearchResult { results }
}

/// Map a raw JSON payload into a typed [`ResearchResult`].
pub fn map_research_payload(payload: serde_json::Value) -> ResearchResult {
    ResearchResult { payload }
}

/// Run a web search via Tavily and return a typed [`SearchResult`].
#[must_use = "search returns a Result that should be handled"]
pub async fn search(
    cfg: &Config,
    query: &str,
    opts: SearchOptions,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<SearchResult, Box<dyn Error>> {
    search_batch(cfg, &[query], opts, tx).await
}

/// Run multiple Tavily searches in sequence and return merged results.
pub async fn search_batch(
    cfg: &Config,
    queries: &[&str],
    opts: SearchOptions,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<SearchResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!(
                "starting search: {}",
                queries
                    .iter()
                    .map(|query| query_log_summary(query, cfg))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        },
    )
    .await;

    let time_range = opts.time_range.map(to_spider_time_range);
    let mut all: Vec<serde_json::Value> = Vec::new();
    for query in queries {
        let mut raw =
            search_results(cfg, query, opts.limit, opts.offset, time_range.clone()).await?;
        all.append(&mut raw);
    }

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("search complete: {} results", all.len()),
        },
    )
    .await;

    Ok(map_search_results(all))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn time_range_all_variants_map_correctly() {
        assert_eq!(to_spider_time_range(ServiceTimeRange::Day), TimeRange::Day);
        assert_eq!(
            to_spider_time_range(ServiceTimeRange::Week),
            TimeRange::Week
        );
        assert_eq!(
            to_spider_time_range(ServiceTimeRange::Month),
            TimeRange::Month
        );
        assert_eq!(
            to_spider_time_range(ServiceTimeRange::Year),
            TimeRange::Year
        );
    }

    #[test]
    fn map_search_results_empty_vec() {
        assert!(map_search_results(vec![]).results.is_empty());
    }

    #[test]
    fn map_search_results_nonempty() {
        let item = json!({"title": "Axon docs", "url": "https://example.com"});
        let result = map_search_results(vec![item.clone()]);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0], item);
    }

    #[test]
    fn map_research_payload_stores_value() {
        let value = json!({"answer": "42", "sources": []});
        assert_eq!(map_research_payload(value.clone()).payload, value);
    }

    #[test]
    fn query_log_summary_redacts_token_like_substrings() {
        let cfg = Config::default();
        let summary = query_log_summary(
            "find docs for sk-testsecret1234567890 and github_pat_1234567890abcdef",
            &cfg,
        );
        assert!(summary.contains("len="));
        assert!(summary.contains("hash="));
        assert!(summary.contains(REDACTED_TOKEN));
        assert!(!summary.contains("sk-testsecret1234567890"));
        assert!(!summary.contains("github_pat_1234567890abcdef"));
    }

    #[tokio::test]
    async fn search_batch_empty_queries_returns_empty() {
        let cfg = Config::default();
        let result = search_batch(
            &cfg,
            &[],
            SearchOptions {
                limit: 10,
                offset: 0,
                time_range: None,
            },
            None,
        )
        .await
        .expect("search_batch with empty queries should not fail");
        assert!(result.results.is_empty());
    }
}

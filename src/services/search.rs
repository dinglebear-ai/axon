mod searxng;
mod synthesis;

pub use synthesis::research;

use crate::core::config::Config;
use crate::services::events::{LogLevel, ServiceEvent, emit};
use crate::services::types::{ResearchResult, SearchOptions, SearchResult, ServiceTimeRange};
use spider_agent::{Agent, SearchOptions as SpiderSearchOptions, TimeRange};
use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};
use tokio::sync::mpsc;

const REDACTED_TOKEN: &str = "[redacted-token]";
/// Maximum (offset + limit) Tavily window. Larger windows are rejected at the
/// service boundary to surface truncation instead of silently returning
/// fewer rows than requested.
pub(crate) const SEARCH_WINDOW_MAX: usize = 100;

pub(super) fn to_spider_time_range(tr: ServiceTimeRange) -> TimeRange {
    match tr {
        ServiceTimeRange::Day => TimeRange::Day,
        ServiceTimeRange::Week => TimeRange::Week,
        ServiceTimeRange::Month => TimeRange::Month,
        ServiceTimeRange::Year => TimeRange::Year,
    }
}

/// Validate Tavily credentials are present.
///
/// Used by both `search` and `research` as a single source of truth for
/// the prereq check, so callers do not need to duplicate the error message.
pub(crate) fn ensure_tavily_configured(cfg: &Config, op: &str) -> Result<(), Box<dyn Error>> {
    if cfg.tavily_api_key.is_empty() {
        return Err(format!(
            "{op} requires TAVILY_API_KEY — set it in .env (run 'axon doctor' to check service connectivity)"
        )
        .into());
    }
    Ok(())
}

/// Reject pagination windows past Tavily's hard cap so callers see a clear
/// error instead of a silently truncated result set.
pub(crate) fn enforce_pagination_window(limit: usize, offset: usize) -> Result<(), Box<dyn Error>> {
    let total = limit.saturating_add(offset);
    if total > SEARCH_WINDOW_MAX {
        return Err(format!(
            "search window too large: limit={limit} + offset={offset} = {total} > {SEARCH_WINDOW_MAX} \
             (Tavily caps results at {SEARCH_WINDOW_MAX} per query); reduce --limit or --offset"
        )
        .into());
    }
    Ok(())
}

/// Build the per-call query log summary.
///
/// In normal (non-debug) mode the preview is rendered with Rust's `{:?}`
/// debug formatter so embedded control characters (newlines, tabs) appear as
/// readable escapes rather than corrupting the log line. The hash and char
/// count remain stable across runs for the same query, enabling grep-based
/// correlation without exposing the query text in plaintext.
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

/// Heuristic redactor used in log previews.
///
/// Splits on whitespace AND common URL/query-string punctuation (`=`, `&`,
/// `;`, `?`, `,`) so that `?key=sk-…` is tokenized as `key` + `sk-…` and
/// the secret-shaped second token gets redacted.
fn redact_token_like_substrings(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let is_sep = |c: char| {
        c.is_whitespace() || matches!(c, '=' | '&' | ';' | '?' | ',' | '(' | ')' | '<' | '>')
    };
    for piece in input.split_inclusive(is_sep) {
        let (token, sep) = match piece.chars().last() {
            Some(c) if is_sep(c) => {
                let cut = piece.len() - c.len_utf8();
                (&piece[..cut], &piece[cut..])
            }
            _ => (piece, ""),
        };
        let trimmed =
            token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-');
        if !trimmed.is_empty() && looks_like_secret_token(trimmed) {
            out.push_str(&token.replace(trimmed, REDACTED_TOKEN));
        } else {
            out.push_str(token);
        }
        out.push_str(sep);
    }
    out
}

/// Recognized secret-token prefixes for log redaction. Defense-in-depth:
/// the 48-char preview cap already bounds exposure, but matching known
/// shapes blocks the obvious case where a user pastes a token directly.
fn looks_like_secret_token(token: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "sk-",
        "sk_test_",
        "sk_live_",
        "pk_test_",
        "pk_live_",
        "ghp_",
        "github_pat_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "atk_",
        "xox",   // Slack tokens (xoxb-, xoxp-, xoxs-, xoxa-)
        "AKIA",  // AWS access key IDs
        "ASIA",  // AWS temporary access key IDs
        "AIza",  // Google API keys
        "ya29.", // Google OAuth access tokens
        "eyJ",   // JWT (base64url-encoded JSON header)
        "tvly-", // Tavily API keys
    ];
    if PREFIXES.iter().any(|p| token.starts_with(p)) {
        return true;
    }
    let lower = token.to_ascii_lowercase();
    if PREFIXES.iter().any(|p| {
        let pl = p.to_ascii_lowercase();
        lower.starts_with(&pl)
    }) {
        return true;
    }
    token.len() >= 20
        && token.chars().any(|c| c.is_ascii_alphabetic())
        && token.chars().any(|c| c.is_ascii_digit())
}

/// Execute a Tavily web search and return raw JSON result items.
pub async fn search_results(
    cfg: &Config,
    query: &str,
    limit: usize,
    offset: usize,
    time_range: Option<TimeRange>,
) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    ensure_tavily_configured(cfg, "search")?;
    enforce_pagination_window(limit, offset)?;
    let total = limit.saturating_add(offset).max(1);
    let mut search_opts = SpiderSearchOptions::new().with_limit(total);
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

/// Wrap a typed [`crate::services::types::ResearchPayload`] in a [`ResearchResult`].
pub fn map_research_payload(payload: crate::services::types::ResearchPayload) -> ResearchResult {
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
#[path = "search_tests.rs"]
mod tests;

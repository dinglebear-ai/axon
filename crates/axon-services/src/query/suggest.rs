//! `suggest`: LLM-proposed crawl-target discovery, ported off legacy
//! `axon_vector::ops::commands::suggest` (issue #298 cutover).
//!
//! Indexed-URL/domain-facet reads now go through `axon-vectors`'
//! `QdrantVectorStore` (`facet` + `scroll_pages`) instead of legacy
//! `axon-vector`'s raw `qdrant_domain_facets`/`qdrant_indexed_urls` REST
//! calls; the LLM completion call reuses
//! [`crate::query::synthesis::completion::run_text_completion`] instead of
//! duplicating the completion-dispatch logic a third time.

use axon_core::config::Config;
use axon_core::http::normalize_url;
use axon_llm::{self as llm, CompletionRequest};
use axon_vectors::qdrant::QdrantVectorStore;
use reqwest::Url;
use std::collections::HashSet;
use std::error::Error;

use crate::query::synthesis::completion::run_text_completion;

/// Provider id tag for the read-only Qdrant store this module constructs
/// directly from `cfg.qdrant_url` (suggest has no dependency on the
/// injected-runtime read stores — it only needs facet/scroll reads).
const SUGGEST_VECTOR_PROVIDER_ID: &str = "axon-services-suggest";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Suggestion {
    url: String,
    reason: String,
}

/// Canonical URL variants to check against the indexed-URL lookup set.
/// Duplicated (not imported) from legacy
/// `axon_vector::ops::input::url_lookup_candidates` — a small, pure,
/// dependency-free helper not worth pulling in the rest of that module for.
fn url_lookup_candidates(target: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let normalized = normalize_url(target);
    let variants = [
        target.to_string(),
        normalized.to_string(),
        normalized.trim_end_matches('/').to_string(),
        format!("{}/", normalized.trim_end_matches('/')),
    ];
    for variant in variants {
        if variant.is_empty() {
            continue;
        }
        if seen.insert(variant.clone()) {
            out.push(variant);
        }
    }
    out
}

fn parse_http_url(value: &str) -> Option<String> {
    let normalized = normalize_url(value.trim());
    let parsed = Url::parse(&normalized).ok()?;
    match parsed.scheme() {
        "http" | "https" => Some(parsed.to_string()),
        _ => None,
    }
}

const DEFAULT_REASON: &str = "Suggested by model";

fn parse_suggestions_from_llm(content: &str) -> Vec<Suggestion> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    let push =
        |out: &mut Vec<Suggestion>, seen: &mut HashSet<String>, url: String, reason: String| {
            if seen.insert(url.clone()) {
                out.push(Suggestion { url, reason });
            }
        };

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(content)
        && let Some(items) = value.get("suggestions").and_then(|v| v.as_array())
    {
        for item in items {
            if let Some(url) = item
                .get("url")
                .and_then(|v| v.as_str())
                .and_then(parse_http_url)
            {
                let reason = item
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or(DEFAULT_REASON)
                    .to_string();
                push(&mut out, &mut seen, url, reason);
            } else if let Some(url) = item.as_str().and_then(parse_http_url) {
                push(&mut out, &mut seen, url, DEFAULT_REASON.to_string());
            }
        }
        return out;
    }

    for token in content.split_whitespace() {
        let cleaned = token
            .trim_matches(|c: char| {
                matches!(
                    c,
                    '"' | '\'' | ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>'
                )
            })
            .trim_end_matches('.');
        if let Some(url) = parse_http_url(cleaned) {
            push(&mut out, &mut seen, url, DEFAULT_REASON.to_string());
        }
    }

    out
}

fn already_indexed(url: &str, indexed_lookup: &HashSet<String>) -> bool {
    url_lookup_candidates(url)
        .iter()
        .any(|v| indexed_lookup.contains(v))
}

fn suggestion_score(url: &str) -> i32 {
    let parsed = Url::parse(url).ok();
    let Some(parsed) = parsed else {
        return 0;
    };
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let path = parsed.path().to_ascii_lowercase();
    let full = format!("{host}{path}");
    let mut score = 0i32;

    let high_value = [
        "docs",
        "reference",
        "api",
        "guide",
        "manual",
        "changelog",
        "release",
        "help",
        "kb",
    ];
    if high_value.iter().any(|k| full.contains(k)) {
        score += 4;
    }
    if path == "/" || path.is_empty() {
        score += 1;
    }
    let depth = path.split('/').filter(|s| !s.is_empty()).count();
    if (1..=4).contains(&depth) {
        score += 2;
    }
    if parsed.query().is_some() {
        score -= 2;
    }
    let low_value = [
        "privacy", "terms", "careers", "press", "blog", "news", "about",
    ];
    if low_value.iter().any(|k| path.contains(k)) {
        score -= 3;
    }
    let binary_suffixes = [".zip", ".gz", ".tar", ".exe", ".dmg"];
    if binary_suffixes.iter().any(|s| path.ends_with(s)) {
        score -= 6;
    }
    score
}

fn host_of(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default()
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

fn indexed_url_from_payload(payload: &serde_json::Value) -> Option<String> {
    ["item_canonical_uri", "source_canonical_uri"]
        .into_iter()
        .find_map(|field| payload_str(payload, field).and_then(parse_http_url))
        .or_else(|| chunk_locator_canonical_uri(payload).and_then(parse_http_url))
}

fn ranked_base_urls_from_context(
    indexed_urls: &[String],
    domain_facets: Vec<(String, u64)>,
) -> Vec<(String, usize)> {
    let mut ranked: Vec<(String, usize)> = domain_facets
        .into_iter()
        .filter(|(domain, _)| !domain.is_empty() && domain != "unknown")
        .map(|(domain, count)| (domain, count as usize))
        .collect();
    if ranked.is_empty() {
        let mut counts = std::collections::BTreeMap::<String, usize>::new();
        for url in indexed_urls {
            let host = host_of(url);
            if !host.is_empty() {
                *counts.entry(host).or_insert(0) += 1;
            }
        }
        ranked = counts.into_iter().collect();
    }
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
}

#[allow(dead_code)]
struct SuggestPromptContext {
    desired: usize,
    /// How many URLs to request from the LLM. Over-samples relative to `desired`
    /// so that after filtering already-indexed URLs the output still reaches `desired`.
    llm_request: usize,
    indexed_urls: Vec<String>,
    indexed_lookup: HashSet<String>,
    ranked_base_urls: Vec<(String, usize)>,
    focus: String,
    base_context: String,
    existing_url_context: String,
}

/// Fetch every distinct indexed canonical URL (one representative chunk per
/// document, via the `chunk_index == 0` filter) up to `limit`. Ports legacy
/// `qdrant_indexed_urls` on top of `QdrantVectorStore::scroll_pages`, but uses
/// the unified vector payload's canonical URI fields first.
async fn fetch_indexed_urls(
    store: &QdrantVectorStore,
    collection: &str,
    limit: Option<usize>,
) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
    let filter = serde_json::json!({ "must": [{ "key": "chunk_index", "match": { "value": 0 } }] });
    let mut seen: HashSet<String> = HashSet::new();
    store
        .scroll_pages(
            collection,
            Some(filter),
            serde_json::json!({ "include": [
                "item_canonical_uri",
                "source_canonical_uri",
                "chunk_locator"
            ] }),
            256,
            |page| {
                for point in page {
                    if let Some(url) = indexed_url_from_payload(&point.payload) {
                        seen.insert(url);
                    }
                }
                limit.is_none_or(|cap| seen.len() < cap)
            },
        )
        .await
        .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
    Ok(seen.into_iter().collect())
}

async fn build_suggest_prompt_context(
    cfg: &Config,
    focus: &str,
    desired: usize,
) -> Result<SuggestPromptContext, Box<dyn Error + Send + Sync>> {
    let base_url_context_limit =
        axon_core::env::env_usize_clamped("AXON_SUGGEST_BASE_URL_LIMIT", 250, 10, 5_000);
    let existing_url_context_limit =
        axon_core::env::env_usize_clamped("AXON_SUGGEST_EXISTING_URL_LIMIT", 500, 0, 5_000);
    let index_dedup_limit =
        axon_core::env::env_usize_clamped("AXON_SUGGEST_INDEX_LIMIT", 50_000, 100, 500_000);

    let store = QdrantVectorStore::new(cfg.qdrant_url.clone(), SUGGEST_VECTOR_PROVIDER_ID);

    // Fetch indexed URLs for duplicate filtering first. The domain facet is only
    // fallback context, so it must not delay a required scan failure.
    let indexed_urls = fetch_indexed_urls(&store, &cfg.collection, Some(index_dedup_limit)).await?;
    let domain_facets = match store
        .facet(&cfg.collection, "web_domain", None, base_url_context_limit)
        .await
        .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })
    {
        Ok(facets) => facets,
        Err(err) => {
            tracing::warn!(error = %err, "suggest: web_domain facet failed; deriving domain context from canonical URLs");
            Vec::new()
        }
    };

    if indexed_urls.is_empty() {
        return Err("No indexed URLs found in Qdrant collection; index a source first".into());
    }

    // Build lookup set: stored URLs are already normalised, so only slash variants needed.
    let mut indexed_lookup = HashSet::with_capacity(indexed_urls.len() * 2);
    for indexed in &indexed_urls {
        let without_slash = indexed.trim_end_matches('/');
        indexed_lookup.insert(without_slash.to_string());
        indexed_lookup.insert(format!("{without_slash}/"));
    }

    // Domain facets come back alphabetically sorted; re-sort by page count
    // descending. When the facet is unavailable (older collection/no index),
    // derive a coarse host count from the canonical URL scan.
    let ranked_base_urls = ranked_base_urls_from_context(&indexed_urls, domain_facets);

    let base_context = ranked_base_urls
        .iter()
        .map(|(domain, pages)| format!("{domain} (pages={pages})"))
        .collect::<Vec<_>>()
        .join("\n");
    let existing_url_context = indexed_urls
        .iter()
        .take(existing_url_context_limit)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    // Over-sample by 3× so that post-filter rejections (already-indexed URLs the LLM
    // couldn't see) don't reduce the final output below `desired`. Capped at 100 to
    // stay within reasonable LLM context limits.
    let llm_request = (desired * 3).min(100);

    Ok(SuggestPromptContext {
        desired,
        llm_request,
        indexed_urls,
        indexed_lookup,
        ranked_base_urls,
        focus: focus.to_string(),
        base_context,
        existing_url_context,
    })
}

fn build_suggest_user_prompt(ctx: &SuggestPromptContext) -> String {
    format!(
        "You are helping expand a documentation crawl set.\n\
Return STRICT JSON only in this shape:\n\
{{\"suggestions\":[{{\"url\":\"https://...\",\"reason\":\"...\"}}]}}\n\n\
Rules:\n\
- Provide exactly {} suggestions.\n\
- Suggest docs/reference/changelog/API/help URLs likely to complement the indexed base URLs.\n\
- Do not suggest any URL from ALREADY_INDEXED_URLS.\n\
- Do not suggest any URL whose domain is already well-covered (high page count in INDEXED_BASE_URLS_WITH_PAGE_COUNTS) unless you are confident the specific path is not yet indexed.\n\
- Prefer new domains or deeply nested sections not represented in ALREADY_INDEXED_URLS.\n\
- Prefer URLs likely to be crawl entrypoints or high-value docs pages.\n\
- Use only absolute http/https URLs.\n\n\
Focus (optional): {}\n\n\
INDEXED_BASE_URLS_WITH_PAGE_COUNTS:\n{}\n\n\
ALREADY_INDEXED_URLS (sample — more may be indexed):\n{}",
        ctx.llm_request, ctx.focus, ctx.base_context, ctx.existing_url_context
    )
}

fn build_suggest_completion_request(cfg: &Config, user_prompt: &str) -> CompletionRequest {
    let req = CompletionRequest::new(user_prompt)
        .system_prompt("You propose complementary documentation source targets. Output JSON only.")
        .stream(false);
    let req = req.backend_from_config(cfg);
    match llm::configured_model_from_config(cfg) {
        Some(model) => req.model(model),
        None => req,
    }
}

async fn request_suggestions_from_llm(
    cfg: &Config,
    user_prompt: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let req = build_suggest_completion_request(cfg, user_prompt);
    run_text_completion(cfg, req)
        .await
        .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })
}

fn filter_new_suggestions(
    content: &str,
    indexed_lookup: &HashSet<String>,
    desired: usize,
) -> (Vec<Suggestion>, Vec<String>) {
    let parsed = parse_suggestions_from_llm(content);
    let mut accepted = Vec::new();
    let mut rejected_existing = Vec::new();
    let mut accepted_seen = HashSet::new();

    for suggestion in parsed {
        if already_indexed(&suggestion.url, indexed_lookup) {
            rejected_existing.push(suggestion.url);
            continue;
        }
        if accepted_seen.insert(suggestion.url.clone()) {
            accepted.push(suggestion);
        }
    }

    // Pre-compute scores to avoid O(n log n) URL parses in the sort comparator.
    let mut scored: Vec<(i32, Suggestion)> = accepted
        .into_iter()
        .map(|s| {
            let score = suggestion_score(&s.url);
            (score, s)
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.url.cmp(&b.1.url)));
    let accepted: Vec<Suggestion> = scored.into_iter().map(|(_, s)| s).collect();
    let mut diversified = Vec::new();
    let mut used_hosts = HashSet::new();
    for suggestion in &accepted {
        let host = host_of(&suggestion.url);
        if host.is_empty() {
            continue;
        }
        if used_hosts.insert(host) {
            diversified.push(suggestion.clone());
            if diversified.len() >= desired {
                break;
            }
        }
    }
    if diversified.len() < desired {
        let mut seen_urls = diversified
            .iter()
            .map(|s| s.url.clone())
            .collect::<HashSet<_>>();
        for suggestion in &accepted {
            if seen_urls.insert(suggestion.url.clone()) {
                diversified.push(suggestion.clone());
            }
            if diversified.len() >= desired {
                break;
            }
        }
    }
    (diversified, rejected_existing)
}

async fn discover_suggestions_with_context(
    cfg: &Config,
    focus: &str,
    desired: usize,
) -> Result<
    (Vec<Suggestion>, Vec<String>, String, SuggestPromptContext),
    Box<dyn Error + Send + Sync>,
> {
    let ctx = build_suggest_prompt_context(cfg, focus, desired).await?;
    let user_prompt = build_suggest_user_prompt(&ctx);
    let content = request_suggestions_from_llm(cfg, &user_prompt).await?;
    let (accepted, rejected_existing) =
        filter_new_suggestions(&content, &ctx.indexed_lookup, desired);
    Ok((accepted, rejected_existing, content, ctx))
}

/// Suggest new URLs to crawl based on the current Qdrant index and an
/// optional focus. Returns `(url, reason)` pairs.
pub(crate) async fn discover_crawl_suggestions(
    cfg: &Config,
    focus: &str,
    desired: usize,
) -> Result<Vec<(String, String)>, Box<dyn Error + Send + Sync>> {
    let desired = desired.clamp(1, 100);
    let (accepted, _, _, _) = discover_suggestions_with_context(cfg, focus, desired).await?;
    Ok(accepted
        .into_iter()
        .map(|s| (s.url, s.reason))
        .collect::<Vec<_>>())
}

#[cfg(test)]
#[path = "suggest_tests.rs"]
mod tests;

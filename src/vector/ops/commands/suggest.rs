use crate::core::config::Config;
use crate::core::http::normalize_url;
use crate::core::logging::log_warn;
use crate::services::acp_llm::{self, AcpCompletionRequest};
use crate::vector::ops::{input, qdrant};
use spider::url::Url;
use std::collections::HashSet;
use std::error::Error;

#[cfg(test)]
use crate::services::acp_llm::AcpCompletionRunner;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Suggestion {
    url: String,
    reason: String,
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
    input::url_lookup_candidates(url)
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

#[expect(
    dead_code,
    reason = "scaffolding for suggest prompt context — wire up before release"
)]
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

async fn build_suggest_prompt_context(
    cfg: &Config,
    focus: &str,
    desired: usize,
) -> Result<SuggestPromptContext, Box<dyn Error>> {
    let base_url_context_limit =
        qdrant::env_usize_clamped("AXON_SUGGEST_BASE_URL_LIMIT", 250, 10, 5_000);
    let existing_url_context_limit =
        qdrant::env_usize_clamped("AXON_SUGGEST_EXISTING_URL_LIMIT", 500, 0, 5_000);
    let index_dedup_limit =
        qdrant::env_usize_clamped("AXON_SUGGEST_INDEX_LIMIT", 50_000, 100, 500_000);

    // Fetch indexed URLs for duplicate filtering (capped to avoid full-collection scan).
    let (indexed_urls, mut ranked_base_urls) = spider::tokio::try_join!(
        qdrant::qdrant_indexed_urls(cfg, Some(index_dedup_limit)),
        qdrant::qdrant_domain_facets(cfg, base_url_context_limit),
    )?;

    if indexed_urls.is_empty() {
        return Err("No indexed URLs found in Qdrant collection; run crawl/scrape first".into());
    }

    // Build lookup set: stored URLs are already normalised, so only slash variants needed.
    let mut indexed_lookup = HashSet::with_capacity(indexed_urls.len() * 2);
    for indexed in &indexed_urls {
        let without_slash = indexed.trim_end_matches('/');
        indexed_lookup.insert(without_slash.to_string());
        indexed_lookup.insert(format!("{without_slash}/"));
    }

    // Domain facets come back alphabetically sorted; re-sort by page count descending.
    ranked_base_urls.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

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
    let user_prompt = format!(
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
    );
    user_prompt
}

fn build_suggest_completion_request(cfg: &Config, user_prompt: &str) -> AcpCompletionRequest {
    let req = AcpCompletionRequest::new(user_prompt)
        .system_prompt("You propose complementary documentation crawl targets. Output JSON only.")
        .stream(false);
    if cfg.openai_model.trim().is_empty() {
        req
    } else {
        req.model(cfg.openai_model.clone())
    }
}

#[cfg(test)]
async fn request_suggestions_from_runner<R>(
    runner: &R,
    user_prompt: &str,
) -> Result<String, Box<dyn Error>>
where
    R: AcpCompletionRunner + ?Sized,
{
    let req = AcpCompletionRequest::new(user_prompt)
        .system_prompt("You propose complementary documentation crawl targets. Output JSON only.")
        .stream(false);
    let response = acp_llm::complete_text_with_runner(runner, req).await?;
    Ok(response.text)
}

async fn request_suggestions_from_llm(
    cfg: &Config,
    user_prompt: &str,
    warm: Option<acp_llm::WarmAcpSession>,
) -> Result<String, Box<dyn Error>> {
    let req = build_suggest_completion_request(cfg, user_prompt);
    // Delegate to run_text_completion which handles warm/cold dispatch and ensures
    // the cold path is confined to a spawn_blocking thread (keeping this future Send).
    super::streaming::run_text_completion(cfg, req, warm).await
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
) -> Result<(Vec<Suggestion>, Vec<String>, String, SuggestPromptContext), Box<dyn Error>> {
    // Start warming before Qdrant calls so cold-start overlaps with retrieval.
    let warm = match acp_llm::warm_session(cfg, None) {
        Ok(w) => Some(w),
        Err(e) => {
            log_warn(&format!(
                "suggest: warm session failed to start, using cold path: {e}"
            ));
            None
        }
    };
    let ctx = build_suggest_prompt_context(cfg, focus, desired).await?;
    let user_prompt = build_suggest_user_prompt(&ctx);
    let content = request_suggestions_from_llm(cfg, &user_prompt, warm).await?;
    let (accepted, rejected_existing) =
        filter_new_suggestions(&content, &ctx.indexed_lookup, desired);
    Ok((accepted, rejected_existing, content, ctx))
}

pub async fn discover_crawl_suggestions(
    cfg: &Config,
    focus: &str,
    desired: usize,
) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let desired = desired.clamp(1, 100);
    let (accepted, _, _, _) = discover_suggestions_with_context(cfg, focus, desired).await?;
    Ok(accepted
        .into_iter()
        .map(|s| (s.url, s.reason))
        .collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::{
        already_indexed, filter_new_suggestions, parse_suggestions_from_llm,
        request_suggestions_from_runner,
    };
    use crate::services::acp_llm::{
        AcpCompletionRequest, AcpCompletionRunner, AcpCompletionTurnResult,
    };
    use std::collections::HashSet;
    use std::error::Error;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct FakeCompletionRunner {
        captured_requests: Arc<Mutex<Vec<AcpCompletionRequest>>>,
        result: AcpCompletionTurnResult,
    }

    #[async_trait::async_trait(?Send)]
    impl AcpCompletionRunner for FakeCompletionRunner {
        async fn complete_text(
            &self,
            req: AcpCompletionRequest,
        ) -> Result<AcpCompletionTurnResult, Box<dyn Error>> {
            self.captured_requests
                .lock()
                .expect("lock request capture")
                .push(req);
            Ok(self.result.clone())
        }

        async fn complete_streaming<F>(
            &self,
            _req: AcpCompletionRequest,
            _on_delta: &mut F,
        ) -> Result<AcpCompletionTurnResult, Box<dyn Error>>
        where
            F: FnMut(&str) -> Result<(), Box<dyn Error>> + Send,
        {
            unreachable!("suggestions request path should use complete_text")
        }
    }

    #[test]
    fn parses_json_suggestions() {
        let input = r#"{
          "suggestions": [
            {"url":"https://docs.example.com/getting-started","reason":"Core onboarding guide"},
            {"url":"https://api.example.com/reference","reason":"API endpoint docs"}
          ]
        }"#;
        let parsed = parse_suggestions_from_llm(input);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].url, "https://docs.example.com/getting-started");
        assert_eq!(parsed[1].url, "https://api.example.com/reference");
    }

    #[test]
    fn parses_url_tokens_when_json_is_missing() {
        let input = "Try https://docs.rs/spider and https://doc.rust-lang.org/book/.";
        let parsed = parse_suggestions_from_llm(input);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].url, "https://docs.rs/spider");
        assert_eq!(parsed[1].url, "https://doc.rust-lang.org/book/");
    }

    #[test]
    fn rejects_already_indexed_url_variants() {
        let mut indexed = HashSet::new();
        indexed.insert("https://docs.example.com/guide".to_string());
        assert!(already_indexed("https://docs.example.com/guide/", &indexed));
        assert!(!already_indexed(
            "https://docs.example.com/changelog",
            &indexed
        ));
    }

    #[test]
    fn filter_prefers_high_value_urls_and_diversifies_hosts() {
        let mut indexed = HashSet::new();
        indexed.insert("https://docs.a.com/old".to_string());
        let content = r#"{
          "suggestions": [
            {"url":"https://a.com/privacy","reason":"low value"},
            {"url":"https://docs.a.com/reference/api","reason":"high value"},
            {"url":"https://docs.b.com/guide","reason":"high value"},
            {"url":"https://a.com/news","reason":"low value"}
          ]
        }"#;
        let (accepted, _rejected) = filter_new_suggestions(content, &indexed, 2);
        assert_eq!(accepted.len(), 2);
        assert_eq!(accepted[0].url, "https://docs.a.com/reference/api");
        assert_eq!(accepted[1].url, "https://docs.b.com/guide");
    }

    #[tokio::test]
    async fn request_suggestions_from_runner_reads_gateway_text() {
        let captured_requests = Arc::new(Mutex::new(Vec::new()));
        let runner = FakeCompletionRunner {
            captured_requests: Arc::clone(&captured_requests),
            result: AcpCompletionTurnResult {
                text: r#"{"suggestions":[{"url":"https://docs.example.com/guide","reason":"ACP gateway text"}]}"#.to_string(),
                usage: None,
            },
        };

        let response = request_suggestions_from_runner(&runner, "docs focus")
            .await
            .expect("runner response should be read");

        assert_eq!(
            response,
            r#"{"suggestions":[{"url":"https://docs.example.com/guide","reason":"ACP gateway text"}]}"#
        );

        let captured = captured_requests.lock().expect("request capture lock");
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].user_prompt, "docs focus");
    }
}

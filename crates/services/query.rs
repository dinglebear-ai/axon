use crate::crates::core::config::Config;
use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::types::{
    AskResult, EvaluateResult, Pagination, QueryResult, RetrieveOptions, RetrieveResult,
    SuggestResult,
};
use crate::crates::vector::ops::commands::ask::ask_payload;
use crate::crates::vector::ops::commands::discover_crawl_suggestions;
use crate::crates::vector::ops::commands::evaluate_payload;
use crate::crates::vector::ops::commands::query_results;
use crate::crates::vector::ops::qdrant::retrieve_result;
use std::error::Error;
use tokio::sync::mpsc;

// ── Pure mapping helpers (unit-testable, no live services required) ──────────

pub fn map_query_results(results: Vec<serde_json::Value>) -> QueryResult {
    QueryResult { results }
}

pub fn map_retrieve_result(chunk_count: usize, content: String) -> RetrieveResult {
    let chunks = if chunk_count == 0 {
        Vec::new()
    } else {
        vec![serde_json::json!({
            "chunk_count": chunk_count,
            "content": content
        })]
    };
    RetrieveResult { chunks }
}

pub fn map_ask_payload(payload: serde_json::Value) -> AskResult {
    AskResult { payload }
}

pub fn map_evaluate_payload(payload: serde_json::Value) -> EvaluateResult {
    EvaluateResult { payload }
}

pub fn map_suggest_payload(payload: &serde_json::Value) -> Result<SuggestResult, Box<dyn Error>> {
    let suggestions = payload
        .get("suggestions")
        .and_then(serde_json::Value::as_array)
        .ok_or("missing suggestions array")?;
    let urls = suggestions
        .iter()
        .enumerate()
        .map(|(i, item)| {
            item.get("url")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
                .ok_or_else(|| format!("suggestions[{i}]: missing url").into())
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    Ok(SuggestResult { urls })
}

// ── Service functions (call-through wrappers) ────────────────────────────────

/// Semantic vector search.
pub async fn query(
    cfg: &Config,
    text: &str,
    opts: Pagination,
) -> Result<QueryResult, Box<dyn Error>> {
    let results = query_results(cfg, text, opts.limit.max(1), opts.offset)
        .await
        .map_err(|e| -> Box<dyn Error> {
            format!(
                "vector query failed for {}: {e}",
                text.chars().take(80).collect::<String>()
            )
            .into()
        })?;
    Ok(map_query_results(results))
}

/// Retrieve stored document chunks for a URL.
pub async fn retrieve(
    cfg: &Config,
    url: &str,
    opts: RetrieveOptions,
) -> Result<RetrieveResult, Box<dyn Error>> {
    let (chunk_count, content) = retrieve_result(cfg, url, opts.max_points)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("retrieve failed for {url}: {e}").into() })?;
    Ok(map_retrieve_result(chunk_count, content))
}

/// RAG ask: retrieve relevant context, then answer with LLM.
pub async fn ask(
    cfg: &Config,
    question: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<AskResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!(
                "starting ask: {}",
                question.chars().take(80).collect::<String>()
            ),
        },
    )
    .await;
    let payload = ask_payload(cfg, question)
        .await
        .map_err(|e| -> Box<dyn Error> {
            format!(
                "ask failed for {}: {e}",
                question.chars().take(80).collect::<String>()
            )
            .into()
        })?;
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "ask complete".to_string(),
        },
    )
    .await;
    Ok(map_ask_payload(payload))
}

/// RAG evaluate: run RAG and baseline answers, then judge with a second LLM call.
///
/// Returns the full structured evaluate payload without printing to stdout.
pub async fn evaluate(cfg: &Config, question: &str) -> Result<EvaluateResult, Box<dyn Error>> {
    let mut derived = cfg.clone();
    derived.query = Some(question.to_string());
    derived.positional = Vec::new();
    let payload = evaluate_payload(&derived)
        .await
        .map_err(|e| -> Box<dyn Error> {
            format!(
                "evaluate failed for {}: {e}",
                question.chars().take(80).collect::<String>()
            )
            .into()
        })?;
    Ok(map_evaluate_payload(payload))
}

/// Suggest new URLs to crawl based on the current Qdrant index and an optional focus.
///
/// Returns the accepted suggestion URLs directly (no stdout side effects).
pub async fn suggest(cfg: &Config, focus: Option<&str>) -> Result<SuggestResult, Box<dyn Error>> {
    let mut derived = cfg.clone();
    derived.query = focus.map(ToString::to_string);
    derived.positional = Vec::new();
    let desired = derived.search_limit.clamp(1, 100);
    let focus_str = focus.unwrap_or_default().to_string();
    let pairs: Vec<(String, String)> = discover_crawl_suggestions(&derived, &focus_str, desired)
        .await
        .map_err(|e| -> Box<dyn Error> {
            format!("crawl suggestion discovery failed: {e}").into()
        })?;
    let urls = pairs.into_iter().map(|(url, _reason)| url).collect();
    Ok(SuggestResult { urls })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── map_retrieve_result ───────────────────────────────────────────────────

    #[test]
    fn map_retrieve_zero_chunks_returns_empty() {
        let result = map_retrieve_result(0, "some content".to_string());
        assert!(
            result.chunks.is_empty(),
            "chunk_count=0 must produce an empty chunks vec; content is discarded"
        );
    }

    #[test]
    fn map_retrieve_nonzero_chunks() {
        let result = map_retrieve_result(5, "hello".to_string());
        assert_eq!(result.chunks.len(), 1);
        assert_eq!(result.chunks[0]["chunk_count"], 5);
        assert_eq!(result.chunks[0]["content"], "hello");
    }

    // ── map_suggest_payload ───────────────────────────────────────────────────

    #[test]
    fn map_suggest_valid() {
        let payload = json!({
            "suggestions": [
                { "url": "https://example.com/a" },
                { "url": "https://example.com/b" }
            ]
        });
        let result = map_suggest_payload(&payload).unwrap();
        assert_eq!(result.urls.len(), 2);
        assert_eq!(result.urls[0], "https://example.com/a");
        assert_eq!(result.urls[1], "https://example.com/b");
    }

    #[test]
    fn map_suggest_missing_suggestions() {
        let payload = json!({});
        let err = map_suggest_payload(&payload).unwrap_err();
        assert!(
            err.to_string().contains("suggestions"),
            "error must mention 'suggestions', got: {err}"
        );
    }

    #[test]
    fn map_suggest_entry_missing_url() {
        let payload = json!({
            "suggestions": [{ "reason": "no url key here" }]
        });
        let err = map_suggest_payload(&payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("suggestions[0]"),
            "error must reference suggestions[0], got: {msg}"
        );
    }

    #[test]
    fn map_suggest_empty_suggestions() {
        let payload = json!({ "suggestions": [] });
        let result = map_suggest_payload(&payload).unwrap();
        assert!(result.urls.is_empty());
    }

    // ── map_query_results ─────────────────────────────────────────────────────

    #[test]
    fn map_query_results_passthrough_empty() {
        let result = map_query_results(vec![]);
        assert!(result.results.is_empty());
    }

    #[test]
    fn map_query_results_passthrough_nonempty() {
        let items = vec![
            json!({ "url": "https://a.com", "score": 0.9 }),
            json!({ "url": "https://b.com", "score": 0.8 }),
        ];
        let result = map_query_results(items.clone());
        assert_eq!(result.results.len(), 2);
        assert_eq!(result.results[0], items[0]);
        assert_eq!(result.results[1], items[1]);
    }
}

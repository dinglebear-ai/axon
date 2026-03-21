use crate::crates::core::config::Config;
use crate::crates::vector::ops::{qdrant, ranking, tei};
use std::error::Error;

const NO_REFERENCE: &str = "No reference material available.";

pub(super) async fn build_judge_reference(
    cfg: &Config,
    question: &str,
) -> Result<(String, usize), Box<dyn Error>> {
    let question_with_instruction = tei::prepend_query_instruction(question);
    let mut vecs = tei::tei_embed(cfg, &[question_with_instruction]).await?;
    if vecs.is_empty() {
        return Err("TEI returned no vector for judge reference".into());
    }
    let query_vec = vecs.remove(0);
    let hits =
        qdrant::dispatch_vector_search(cfg, &query_vec, question, cfg.ask_candidate_limit * 2)
            .await
            .map_err(|e| -> Box<dyn Error> {
                format!("vector search dispatch for evaluate judge reference: {e}").into()
            })?;
    let query_tokens = ranking::tokenize_query(question);
    let mut candidates: Vec<ranking::AskCandidate> = Vec::new();
    for hit in hits {
        let url = qdrant::payload_url_typed(&hit.payload).to_string();
        let chunk_text = qdrant::payload_text_typed(&hit.payload).to_string();
        if url.is_empty() || chunk_text.len() < 40 {
            continue;
        }
        let path = ranking::extract_path_from_url(&url);
        candidates.push(ranking::AskCandidate {
            score: hit.score,
            url: url.clone(),
            path: path.clone(),
            chunk_text: chunk_text.clone(),
            url_tokens: ranking::tokenize_path_set(&path),
            chunk_tokens: ranking::tokenize_text_set(&chunk_text),
            rerank_score: hit.score,
        });
    }
    if candidates.is_empty() {
        return Ok((NO_REFERENCE.to_string(), 0));
    }
    let reranked: Vec<ranking::AskCandidate> = ranking::rerank_ask_candidates(
        &candidates,
        &query_tokens,
        &cfg.ask_authoritative_domains,
        cfg.ask_authoritative_boost,
    )
    .into_iter()
    .filter(|c| c.rerank_score >= cfg.ask_min_relevance_score)
    .collect();
    if reranked.is_empty() {
        return Ok((NO_REFERENCE.to_string(), 0));
    }
    let selected_indices = ranking::select_diverse_candidates(&reranked, 8, 2);
    let selected_count = selected_indices.len();
    let reference = selected_indices
        .iter()
        .enumerate()
        .map(|(i, &idx)| {
            format!(
                "## Reference [R{}]: {}\n\n{}\n\n---",
                i + 1,
                reranked[idx].url,
                reranked[idx].chunk_text
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok((reference, selected_count))
}

pub(super) fn parse_first_score(value: &str, label: &str) -> Option<f64> {
    let start = value.find(label)?;
    let tail = &value[start + label.len()..];
    let mut number = String::new();
    let mut seen = false;
    for ch in tail.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            number.push(ch);
            seen = true;
            continue;
        }
        if seen {
            break;
        }
    }
    number.parse::<f64>().ok()
}

pub(super) fn score_totals_from_analysis(analysis: &str) -> Option<(f64, f64)> {
    let mut rag_total = 0.0f64;
    let mut baseline_total = 0.0f64;
    let mut score_rows = 0usize;
    for line in analysis.lines() {
        let rag = parse_first_score(line, "RAG: ");
        let base = parse_first_score(line, "Baseline: ");
        if let (Some(r), Some(b)) = (rag, base) {
            rag_total += r;
            baseline_total += b;
            score_rows += 1;
        }
    }
    if score_rows == 0 {
        return None;
    }
    Some((rag_total, baseline_total))
}

pub(super) fn rag_underperformed(analysis: &str) -> bool {
    score_totals_from_analysis(analysis)
        .map(|(rag, baseline)| rag + 0.001 < baseline)
        .unwrap_or(false)
}

pub(super) fn build_suggestion_focus(query: &str, analysis: &str) -> String {
    let mut weak_dimensions = Vec::new();
    for line in analysis.lines() {
        let rag = parse_first_score(line, "RAG: ");
        let base = parse_first_score(line, "Baseline: ");
        if let (Some(r), Some(b)) = (rag, base)
            && r + 0.001 < b
        {
            weak_dimensions.push(line.trim().to_string());
        }
    }
    if weak_dimensions.is_empty() {
        return query.to_string();
    }
    format!(
        "{query}\n\nRAG scored below baseline in these areas:\n- {}",
        weak_dimensions.join("\n- ")
    )
}

pub(super) fn format_rag_sources(diagnostic_sources: &[String]) -> String {
    if diagnostic_sources.is_empty() {
        return "None available".to_string();
    }
    diagnostic_sources
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let url = s.split_once(" url=").map_or(s.as_str(), |(_, u)| u);
            format!("[S{}] {}", i + 1, url)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn extract_source_urls(diagnostic_sources: &[String]) -> Vec<String> {
    diagnostic_sources
        .iter()
        .map(|s| {
            s.split_once(" url=")
                .map_or_else(|| s.to_string(), |(_, u)| u.to_string())
        })
        .collect()
}

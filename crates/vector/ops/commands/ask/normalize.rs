use crate::crates::core::config::Config;
use crate::crates::vector::ops::ranking;
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub(crate) fn strip_sources_section(answer: &str) -> String {
    let lower = answer.to_ascii_lowercase();
    if lower.starts_with("## sources") {
        return String::new();
    }
    if let Some(idx) = lower.find("\n## sources") {
        return answer[..idx].trim_end().to_string();
    }
    answer.trim_end().to_string()
}

pub(crate) fn extract_cited_source_ids(text: &str) -> BTreeSet<usize> {
    let bytes = text.as_bytes();
    let mut out = BTreeSet::new();
    let mut i = 0usize;
    while i + 3 < bytes.len() {
        if bytes[i] == b'[' && (bytes[i + 1] == b'S' || bytes[i + 1] == b's') {
            let mut j = i + 2;
            let mut value: usize = 0;
            let mut saw_digit = false;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                saw_digit = true;
                value = value
                    .saturating_mul(10)
                    .saturating_add((bytes[j] - b'0') as usize);
                j += 1;
            }
            if saw_digit && j < bytes.len() && bytes[j] == b']' {
                out.insert(value);
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

pub(crate) fn parse_context_source_map(context: &str) -> BTreeMap<usize, String> {
    let mut out = BTreeMap::new();
    for line in context.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("## ") {
            continue;
        }
        let Some(start) = trimmed.find("[S") else {
            continue;
        };
        let rest = &trimmed[start + 2..];
        let Some(end_rel) = rest.find(']') else {
            continue;
        };
        let id_raw = &rest[..end_rel];
        let Ok(id) = id_raw.parse::<usize>() else {
            continue;
        };
        let Some(colon_idx) = trimmed.find(": ") else {
            continue;
        };
        let source = trimmed[colon_idx + 2..].trim();
        if !source.is_empty() {
            out.insert(id, source.to_string());
        }
    }
    out
}

fn indicates_insufficient_evidence(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("insufficient")
        || lower.contains("not enough information")
        || lower.contains("does not contain information")
        || lower.contains("no relevant information")
}

fn is_non_trivial(query: &str, body: &str) -> bool {
    let query_tokens = ranking::tokenize_query(query);
    let body_words = body.split_whitespace().count();
    query_tokens.len() >= 4 || body_words >= 70 || body.len() >= 450
}

fn format_insufficient_evidence(
    source_map: &BTreeMap<usize, String>,
    cited: Option<&BTreeSet<usize>>,
    reasons: &[String],
) -> String {
    let suggestions = source_map
        .values()
        .take(3)
        .map(|source| format!("- Index authoritative documentation for: {source}"))
        .collect::<Vec<_>>();
    let suggestions_block = if suggestions.is_empty() {
        "- Index official product documentation and command reference pages for this topic."
            .to_string()
    } else {
        suggestions.join("\n")
    };
    let mut seen_sources: HashSet<String> = HashSet::new();
    let source_lines = cited
        .map(|ids| {
            ids.iter()
                .filter_map(|id| {
                    source_map.get(id).and_then(|source| {
                        if seen_sources.insert(source.clone()) {
                            Some(format!("- [S{id}] {source}"))
                        } else {
                            None
                        }
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let sources_block = if source_lines.is_empty() {
        "- None cited from retrieved context.".to_string()
    } else {
        source_lines.join("\n")
    };
    let why_lines = if reasons.is_empty() {
        "- Retrieved context did not contain a direct, source-grounded answer.".to_string()
    } else {
        reasons
            .iter()
            .map(|reason| format!("- {reason}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "Insufficient evidence in indexed sources to answer this question reliably.\n\n\
## Why\n\
{why_lines}\n\n\
## Next Index Targets\n\
{suggestions_block}\n\n\
## Sources\n\
{sources_block}"
    )
}

pub(crate) fn normalize_ask_answer(
    cfg: &Config,
    query: &str,
    answer: &str,
    context: &str,
) -> String {
    let source_map = parse_context_source_map(context);
    let body = strip_sources_section(answer);
    let cited = extract_cited_source_ids(&body);
    let mut insufficiency_reasons: Vec<String> = Vec::new();

    // Gate 1: no citations at all
    if cited.is_empty() {
        insufficiency_reasons.push("Answer contained no source citations.".to_string());
        return format_insufficient_evidence(&source_map, None, &insufficiency_reasons);
    }

    // Gate 2: LLM self-flagged insufficient evidence
    if indicates_insufficient_evidence(&body) {
        insufficiency_reasons.push("Model flagged insufficient supporting evidence.".to_string());
        return format_insufficient_evidence(&source_map, Some(&cited), &insufficiency_reasons);
    }

    // Gate 3: citations don't map to retrieved sources
    let mut seen_sources: HashSet<String> = HashSet::new();
    let source_lines = cited
        .iter()
        .filter_map(|id| {
            source_map.get(id).and_then(|source| {
                if seen_sources.insert(source.clone()) {
                    Some(format!("- [S{id}] {source}"))
                } else {
                    None
                }
            })
        })
        .collect::<Vec<_>>();
    if source_lines.is_empty() {
        insufficiency_reasons.push("Citations did not map to retrieved sources.".to_string());
        return format_insufficient_evidence(&source_map, Some(&cited), &insufficiency_reasons);
    }

    // Gate 4: non-trivial answers need minimum unique citations
    let min_citations = if is_non_trivial(query, &body) {
        cfg.ask_min_citations_nontrivial
    } else {
        1
    };
    if source_lines.len() < min_citations {
        insufficiency_reasons.push(format!(
            "Non-trivial answer requires at least {min_citations} unique citations; found {}.",
            source_lines.len()
        ));
    }

    if !insufficiency_reasons.is_empty() {
        return format_insufficient_evidence(&source_map, Some(&cited), &insufficiency_reasons);
    }

    format!(
        "{}\n\n## Sources\n{}",
        body.trim_end(),
        source_lines.join("\n")
    )
}

use crate::ops::ranking;
use axon_core::config::Config;
use spider::url::Url;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CitationValidationSummary {
    pub(crate) valid: bool,
    pub(crate) issues: Vec<String>,
    pub(crate) canonical_citation_count: usize,
}

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

pub fn summarize_citation_validation(answer: &str) -> CitationValidationSummary {
    let issues = extract_validation_issues(answer);
    let valid = issues.is_empty() && !answer.trim_start().starts_with("Insufficient evidence");
    CitationValidationSummary {
        valid,
        issues,
        canonical_citation_count: count_canonical_source_lines(answer),
    }
}

fn extract_validation_issues(answer: &str) -> Vec<String> {
    let Some(idx) = answer.find("## Citation Validation Failed") else {
        if answer.trim_start().starts_with("Insufficient evidence") {
            return vec!["Insufficient evidence in indexed sources.".to_string()];
        }
        return Vec::new();
    };
    answer[idx + "## Citation Validation Failed".len()..]
        .lines()
        .map(str::trim)
        .take_while(|line| !line.starts_with("## "))
        .filter_map(|line| line.strip_prefix("- ").map(str::trim))
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn count_canonical_source_lines(answer: &str) -> usize {
    let mut in_sources = false;
    let mut count = 0usize;
    for line in answer.lines() {
        let trimmed = line.trim_start();
        if trimmed == "## Sources" || trimmed == "## Retrieved Sources" {
            in_sources = true;
            continue;
        }
        if in_sources && trimmed.starts_with("## ") {
            break;
        }
        if in_sources && trimmed.starts_with("- [S") && trimmed.contains(']') {
            count += 1;
        }
    }
    count
}

pub(crate) fn extract_cited_source_ids(text: &str) -> BTreeSet<usize> {
    let bytes = text.as_bytes();
    let mut out = BTreeSet::new();
    let mut i = 0usize;
    while i + 3 < bytes.len() {
        if bytes[i] == b'['
            && let Some(j) = find_source_citation_end(bytes, i)
        {
            for id in parse_source_citation_ids(&text[i + 1..j]) {
                out.insert(id);
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    out
}

fn find_source_citation_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let mut saw_source = false;
    while i < bytes.len() {
        match bytes[i] {
            b']' => return saw_source.then_some(i),
            b'S' | b's' if i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() => {
                saw_source = true;
                i += 2;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            b',' | b' ' | b'\t' => i += 1,
            _ => return None,
        }
    }
    None
}

fn parse_source_citation_ids(content: &str) -> Vec<usize> {
    let bytes = content.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] == b'S' || bytes[i] == b's' {
            let mut j = i + 1;
            let mut value: usize = 0;
            let mut saw_digit = false;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                saw_digit = true;
                value = value
                    .saturating_mul(10)
                    .saturating_add((bytes[j] - b'0') as usize);
                j += 1;
            }
            if saw_digit {
                out.push(value);
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

fn remap_source_citations(text: &str, id_map: &BTreeMap<usize, usize>) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;
    let mut i = 0usize;
    while i + 3 < bytes.len() {
        if bytes[i] == b'['
            && let Some(j) = find_source_citation_end(bytes, i)
        {
            let display_ids = parse_source_citation_ids(&text[i + 1..j])
                .into_iter()
                .filter_map(|id| id_map.get(&id).copied())
                .collect::<BTreeSet<_>>();
            if !display_ids.is_empty() {
                out.push_str(&text[last..i]);
                out.push('[');
                out.push_str(
                    &display_ids
                        .into_iter()
                        .map(|id| format!("S{id}"))
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                out.push(']');
                i = j + 1;
                last = i;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&text[last..]);
    out
}

fn indicates_insufficient_evidence(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("insufficient")
        || lower.contains("not enough information")
        || lower.contains("does not contain information")
        || lower.contains("no relevant information")
}

fn canonical_source_identity(source: &str) -> String {
    let Ok(parsed) = Url::parse(source) else {
        return strip_route_variant_suffix(source.trim_end_matches('/')).to_ascii_lowercase();
    };
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let mut path = parsed.path().trim_end_matches('/').to_ascii_lowercase();
    if let Some(stripped) = path
        .strip_suffix("/index.html")
        .or_else(|| path.strip_suffix("/index"))
    {
        path = stripped.to_string();
    }
    path = strip_route_variant_suffix(&path).to_string();
    format!("{host}{path}")
}

fn strip_route_variant_suffix(path: &str) -> &str {
    path.strip_suffix(".md")
        .or_else(|| path.strip_suffix(".mdx"))
        .or_else(|| path.strip_suffix(".html"))
        .unwrap_or(path)
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
                        if seen_sources.insert(canonical_source_identity(source)) {
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

fn format_validation_failure(
    body: &str,
    source_map: &BTreeMap<usize, String>,
    cited: Option<&BTreeSet<usize>>,
    reasons: &[String],
) -> String {
    let why_lines = reasons
        .iter()
        .map(|reason| format!("- {reason}"))
        .collect::<Vec<_>>()
        .join("\n");
    let cited_filter = cited.filter(|ids| !ids.is_empty());
    let mut seen_sources: HashSet<String> = HashSet::new();
    let source_lines = match cited_filter {
        Some(ids) => ids
            .iter()
            .filter_map(|id| {
                source_map.get(id).and_then(|source| {
                    if seen_sources.insert(canonical_source_identity(source)) {
                        Some(format!("- [S{id}] {source}"))
                    } else {
                        None
                    }
                })
            })
            .collect::<Vec<_>>(),
        None => source_map
            .iter()
            .take(8)
            .filter_map(|(id, source)| {
                if seen_sources.insert(canonical_source_identity(source)) {
                    Some(format!("- [S{id}] {source}"))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>(),
    };
    let sources_block = if source_lines.is_empty() {
        "- No cited source IDs mapped to retrieved context.".to_string()
    } else {
        source_lines.join("\n")
    };
    format!(
        "{}\n\n## Citation Validation Failed\n{}\n\n## Retrieved Sources\n{}",
        body.trim_end(),
        why_lines,
        sources_block
    )
}

pub fn normalize_ask_answer(cfg: &Config, query: &str, answer: &str, context: &str) -> String {
    let source_map = parse_context_source_map(context);
    let body = strip_sources_section(answer);
    let cited = extract_cited_source_ids(&body);
    let mut insufficiency_reasons: Vec<String> = Vec::new();

    // Gate 1: no citations at all
    if cited.is_empty() {
        insufficiency_reasons.push("Answer contained no source citations.".to_string());
        if source_map.is_empty() {
            return format_insufficient_evidence(&source_map, None, &insufficiency_reasons);
        }
        return format_validation_failure(&body, &source_map, None, &insufficiency_reasons);
    }

    // Gate 2: LLM self-flagged insufficient evidence
    if indicates_insufficient_evidence(&body) {
        insufficiency_reasons.push("Model flagged insufficient supporting evidence.".to_string());
        return format_insufficient_evidence(&source_map, Some(&cited), &insufficiency_reasons);
    }

    // Gate 3: citations don't map to retrieved sources
    let mut seen_sources: HashSet<String> = HashSet::new();
    let mut display_id_by_source: HashMap<String, usize> = HashMap::new();
    let mut display_id_by_original_id: BTreeMap<usize, usize> = BTreeMap::new();
    let mut source_lines = Vec::new();
    for id in cited.iter() {
        let Some(source) = source_map.get(id) else {
            continue;
        };
        let source_identity = canonical_source_identity(source);
        if let Some(display_id) = display_id_by_source.get(&source_identity) {
            display_id_by_original_id.insert(*id, *display_id);
            continue;
        }
        if !seen_sources.insert(source_identity.clone()) {
            continue;
        }
        let display_id = source_lines.len() + 1;
        display_id_by_source.insert(source_identity, display_id);
        display_id_by_original_id.insert(*id, display_id);
        source_lines.push(format!("- [S{display_id}] {source}"));
    }
    if source_lines.is_empty() {
        insufficiency_reasons.push("Citations did not map to retrieved sources.".to_string());
        if source_map.is_empty() {
            return format_insufficient_evidence(&source_map, Some(&cited), &insufficiency_reasons);
        }
        return format_validation_failure(&body, &source_map, Some(&cited), &insufficiency_reasons);
    }

    // Gate 4: non-trivial answers need minimum unique citations
    let cites_follow_up_history = cfg.ask_follow_up
        && cited.iter().any(|id| {
            source_map
                .get(id)
                .is_some_and(|source| source.starts_with("axon ask session:"))
        });
    let min_citations = if cites_follow_up_history {
        1
    } else if is_non_trivial(query, &body) {
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
        return format_validation_failure(&body, &source_map, Some(&cited), &insufficiency_reasons);
    }

    let body = remap_source_citations(&body, &display_id_by_original_id);

    format!(
        "{}\n\n## Sources\n{}",
        body.trim_end(),
        source_lines.join("\n")
    )
}

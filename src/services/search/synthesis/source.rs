use crate::core::config::Config;
use crate::core::llm;
use crate::services::types::{
    ResearchExtraction, SourceInstructionTrust, SourceReputation, SourceType,
};

use super::RawHit;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SourceMeta {
    pub source_type: SourceType,
    pub source_reputation: SourceReputation,
    pub instruction_trust: SourceInstructionTrust,
}

pub(super) fn build_extraction(
    cfg: &Config,
    hit: &RawHit,
    full_content: Option<&str>,
    per_source: usize,
) -> ResearchExtraction {
    let full = full_content.unwrap_or("");
    let content = if full.trim().chars().count() > hit.snippet.trim().chars().count() {
        full
    } else {
        hit.snippet.as_str()
    };
    let trimmed = content.trim();
    let extracted = if preserve_full_research_sources(cfg) {
        trimmed.to_string()
    } else {
        truncate_chars(trimmed, per_source).to_string()
    };
    let meta = classify_source(&hit.url, &hit.title);
    ResearchExtraction {
        url: hit.url.clone(),
        title: hit.title.clone(),
        extracted,
        source_type: meta.source_type,
        source_reputation: meta.source_reputation,
        instruction_trust: meta.instruction_trust,
        relevance_score: None,
    }
}

fn preserve_full_research_sources(cfg: &Config) -> bool {
    use crate::core::llm::LlmBackendKind;
    let configured = llm::configured_model_from_config(cfg)
        .unwrap_or_else(|| cfg.openai_model.clone())
        .to_ascii_lowercase();
    matches!(cfg.llm_backend, LlmBackendKind::GeminiHeadless)
        || configured.contains("gemini")
        || configured.contains("opus")
}

pub(super) fn classify_source(url: &str, title: &str) -> SourceMeta {
    let parsed = url::Url::parse(url).ok();
    let host = parsed
        .as_ref()
        .and_then(|u| u.host_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let path = parsed
        .as_ref()
        .map(|u| u.path().to_ascii_lowercase())
        .unwrap_or_default();
    let title = title.to_ascii_lowercase();

    let source_type = if host.starts_with("docs.")
        || host.contains(".docs.")
        || path.contains("/docs")
        || title.contains("docs")
        || host.ends_with("readthedocs.io")
    {
        SourceType::OfficialDocs
    } else if host == "github.com" || host.ends_with(".github.io") {
        SourceType::Repository
    } else if host.contains("forum") || host.contains("community") || host.contains("reddit.com") {
        SourceType::Forum
    } else if host.contains("news") || host.contains("blog") || path.contains("/blog") {
        SourceType::Blog
    } else {
        SourceType::Unknown
    };

    let source_reputation = match source_type {
        SourceType::OfficialDocs | SourceType::ReferenceDocs => SourceReputation::Authoritative,
        SourceType::Repository => SourceReputation::High,
        SourceType::Blog | SourceType::News => SourceReputation::Medium,
        SourceType::Forum => SourceReputation::Low,
        SourceType::Unknown => SourceReputation::Unknown,
    };

    SourceMeta {
        source_type,
        source_reputation,
        instruction_trust: SourceInstructionTrust::EvidenceOnly,
    }
}

pub(super) fn rank_relevant_extractions(
    query: &str,
    mut extractions: Vec<ResearchExtraction>,
) -> Vec<ResearchExtraction> {
    let terms: Vec<String> = query
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|term| term.len() >= 3)
        .map(|term| term.to_ascii_lowercase())
        .collect();
    if terms.is_empty() {
        return extractions;
    }
    for extraction in &mut extractions {
        let haystack =
            format!("{} {}", extraction.title, extraction.extracted).to_ascii_lowercase();
        let hits = terms
            .iter()
            .filter(|term| haystack.contains(term.as_str()))
            .count();
        let score = ((hits * 100) / terms.len()).min(100) as u8;
        extraction.relevance_score = Some(score);
    }
    extractions.sort_by(|a, b| {
        b.relevance_score
            .unwrap_or(0)
            .cmp(&a.relevance_score.unwrap_or(0))
            .then_with(|| {
                reputation_rank(b.source_reputation).cmp(&reputation_rank(a.source_reputation))
            })
    });
    extractions
}

fn reputation_rank(reputation: SourceReputation) -> u8 {
    match reputation {
        SourceReputation::Authoritative => 5,
        SourceReputation::High => 4,
        SourceReputation::Medium => 3,
        SourceReputation::Low => 2,
        SourceReputation::Unknown => 1,
    }
}

/// Truncate to at most `max_chars` characters on a char boundary.
pub(super) fn truncate_chars(value: &str, max_chars: usize) -> &str {
    match value.char_indices().nth(max_chars) {
        Some((idx, _)) => &value[..idx],
        None => value,
    }
}

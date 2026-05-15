use crate::services::types::{
    AskExplainContextSource, AskExplainSelectionDecision, AskExplainSelectionDecisionKind,
};
use crate::vector::ops::ranking;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(in crate::vector::ops::commands::ask::context) struct ContextCandidateSelection {
    pub(in crate::vector::ops::commands::ask::context) candidate_index: usize,
    pub(in crate::vector::ops::commands::ask::context) url: String,
    pub(in crate::vector::ops::commands::ask::context) decisions: Vec<AskExplainSelectionDecision>,
}

pub(crate) struct ContextSelectionInputs<'a> {
    pub(crate) reranked: &'a [ranking::AskCandidate],
    pub(crate) top_chunk_indices: &'a [usize],
    pub(crate) selected_top_chunk_indices: &'a [usize],
    pub(crate) planned_full_doc_urls: &'a HashSet<String>,
    pub(crate) top_full_doc_indices: &'a [usize],
    pub(crate) inserted_full_doc_urls: &'a HashSet<String>,
    pub(crate) supplemental_indices: &'a [usize],
    pub(crate) supplemental_count: usize,
    pub(crate) full_doc_fetch_skipped: bool,
}

pub(crate) fn build_context_selection_decisions(
    inputs: ContextSelectionInputs<'_>,
) -> Vec<ContextCandidateSelection> {
    let top_chunk_set = index_set(inputs.top_chunk_indices);
    let selected_top_chunk_set = index_set(inputs.selected_top_chunk_indices);
    let top_full_doc_set = index_set(inputs.top_full_doc_indices);
    let supplemental_selected = inputs
        .supplemental_indices
        .iter()
        .copied()
        .take(inputs.supplemental_count)
        .collect::<HashSet<_>>();
    let supplemental_budget_skipped = inputs
        .supplemental_indices
        .iter()
        .copied()
        .skip(inputs.supplemental_count)
        .collect::<HashSet<_>>();

    inputs
        .reranked
        .iter()
        .enumerate()
        .map(|(idx, candidate)| {
            let decisions = candidate_selection_decisions(CandidateDecisionInputs {
                idx,
                candidate,
                top_chunk_set: &top_chunk_set,
                selected_top_chunk_set: &selected_top_chunk_set,
                planned_full_doc_urls: inputs.planned_full_doc_urls,
                top_full_doc_set: &top_full_doc_set,
                inserted_full_doc_urls: inputs.inserted_full_doc_urls,
                supplemental_selected: &supplemental_selected,
                supplemental_budget_skipped: &supplemental_budget_skipped,
                full_doc_fetch_skipped: inputs.full_doc_fetch_skipped,
            });
            ContextCandidateSelection {
                candidate_index: idx,
                url: candidate.url.clone(),
                decisions,
            }
        })
        .collect()
}

struct CandidateDecisionInputs<'a> {
    idx: usize,
    candidate: &'a ranking::AskCandidate,
    top_chunk_set: &'a HashSet<usize>,
    selected_top_chunk_set: &'a HashSet<usize>,
    planned_full_doc_urls: &'a HashSet<String>,
    top_full_doc_set: &'a HashSet<usize>,
    inserted_full_doc_urls: &'a HashSet<String>,
    supplemental_selected: &'a HashSet<usize>,
    supplemental_budget_skipped: &'a HashSet<usize>,
    full_doc_fetch_skipped: bool,
}

fn candidate_selection_decisions(
    inputs: CandidateDecisionInputs<'_>,
) -> Vec<AskExplainSelectionDecision> {
    let mut decisions = Vec::new();
    push_top_chunk_decision(&mut decisions, &inputs);
    push_full_doc_decisions(&mut decisions, &inputs);
    push_supplemental_decision(&mut decisions, &inputs);
    if decisions.is_empty() {
        decisions.push(selection_decision(
            AskExplainSelectionDecisionKind::NotSelected,
            None,
        ));
    }
    decisions
}

fn push_top_chunk_decision(
    decisions: &mut Vec<AskExplainSelectionDecision>,
    inputs: &CandidateDecisionInputs<'_>,
) {
    if inputs.selected_top_chunk_set.contains(&inputs.idx) {
        decisions.push(selection_decision(
            AskExplainSelectionDecisionKind::SelectedTopChunk,
            None,
        ));
    } else if inputs.top_chunk_set.contains(&inputs.idx)
        && inputs.planned_full_doc_urls.contains(&inputs.candidate.url)
    {
        decisions.push(selection_decision(
            AskExplainSelectionDecisionKind::SkippedPlannedFullDoc,
            Some("top chunk was suppressed because the same URL was planned for full-doc fetch"),
        ));
    } else if inputs.top_chunk_set.contains(&inputs.idx) {
        decisions.push(selection_decision(
            AskExplainSelectionDecisionKind::SkippedBudget,
            Some("top chunk was not inserted before the context budget filled"),
        ));
    }
}

fn push_full_doc_decisions(
    decisions: &mut Vec<AskExplainSelectionDecision>,
    inputs: &CandidateDecisionInputs<'_>,
) {
    if !inputs.top_full_doc_set.contains(&inputs.idx) {
        return;
    }
    decisions.push(selection_decision(
        AskExplainSelectionDecisionKind::PlannedFullDoc,
        None,
    ));
    if inputs.full_doc_fetch_skipped {
        decisions.push(selection_decision(
            AskExplainSelectionDecisionKind::SkippedFullDocFetchSkipped,
            Some("adaptive gate skipped full-doc fetch for this request"),
        ));
    } else if inputs
        .inserted_full_doc_urls
        .contains(&inputs.candidate.url)
    {
        decisions.push(selection_decision(
            AskExplainSelectionDecisionKind::InsertedFullDoc,
            None,
        ));
    } else {
        decisions.push(selection_decision(
            AskExplainSelectionDecisionKind::SkippedBudget,
            Some("planned full document was not inserted before the context budget filled"),
        ));
    }
}

fn push_supplemental_decision(
    decisions: &mut Vec<AskExplainSelectionDecision>,
    inputs: &CandidateDecisionInputs<'_>,
) {
    if inputs.supplemental_selected.contains(&inputs.idx) {
        decisions.push(selection_decision(
            AskExplainSelectionDecisionKind::SelectedSupplemental,
            None,
        ));
    } else if inputs.supplemental_budget_skipped.contains(&inputs.idx) {
        decisions.push(selection_decision(
            AskExplainSelectionDecisionKind::SkippedBudget,
            Some("supplemental chunk was not inserted before the context budget filled"),
        ));
    }
}

fn selection_decision(
    kind: AskExplainSelectionDecisionKind,
    reason: Option<&str>,
) -> AskExplainSelectionDecision {
    AskExplainSelectionDecision {
        kind,
        reason: reason.map(str::to_string),
    }
}

pub(crate) fn selected_top_chunk_indices(
    reranked: &[ranking::AskCandidate],
    top_chunk_indices: &[usize],
    planned_full_doc_urls: &HashSet<String>,
    top_chunks_selected: usize,
) -> Vec<usize> {
    top_chunk_indices
        .iter()
        .copied()
        .filter(|&idx| {
            reranked
                .get(idx)
                .is_some_and(|candidate| !planned_full_doc_urls.contains(&candidate.url))
        })
        .take(top_chunks_selected)
        .collect()
}

pub(crate) fn context_source_candidate_count(
    reranked: &[ranking::AskCandidate],
    top_chunk_indices: &[usize],
    planned_full_doc_urls: &HashSet<String>,
    top_full_doc_indices: &[usize],
    supplemental: &[usize],
    full_doc_fetch_skipped: bool,
) -> usize {
    let chunk_candidates = top_chunk_indices
        .iter()
        .filter(|&&idx| {
            reranked
                .get(idx)
                .is_some_and(|candidate| !planned_full_doc_urls.contains(&candidate.url))
        })
        .count();
    let full_doc_candidates = if full_doc_fetch_skipped {
        0
    } else {
        top_full_doc_indices.len()
    };
    chunk_candidates + full_doc_candidates + supplemental.len()
}

pub(crate) fn sorted_urls(urls: &HashSet<String>) -> Vec<String> {
    let mut urls = urls.iter().cloned().collect::<Vec<_>>();
    urls.sort();
    urls
}

pub(crate) fn final_source_order_from_context(context: &str) -> Vec<AskExplainContextSource> {
    context.lines().filter_map(parse_source_header).collect()
}

fn parse_source_header(line: &str) -> Option<AskExplainContextSource> {
    let line = line.strip_prefix("## ")?;
    let (tier_text, rest) = line.split_once(" [")?;
    let tier = match tier_text {
        "Top Chunk" => "top_chunk",
        "Source Document" => "full_doc",
        "Supplemental Chunk" => "supplemental",
        _ => return None,
    };
    let (source_id, rest) = rest.split_once("]: ")?;
    Some(AskExplainContextSource {
        source_id: source_id.to_string(),
        url: rest.to_string(),
        tier: tier.to_string(),
    })
}

fn index_set(indices: &[usize]) -> HashSet<usize> {
    indices.iter().copied().collect()
}

#[cfg(test)]
#[path = "trace/tests.rs"]
mod tests;

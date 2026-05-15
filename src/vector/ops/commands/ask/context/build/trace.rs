use crate::services::types::{
    AskExplainContextSource, AskExplainInsertionMode, AskExplainSelectionDecision,
    AskExplainSelectionDecisionKind,
};
use crate::vector::ops::ranking;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::vector::ops::commands::ask::context) struct CandidateSelectionKey {
    url: String,
    chunk_text: String,
}

pub(in crate::vector::ops::commands::ask::context) fn candidate_selection_key(
    candidate: &ranking::AskCandidate,
) -> CandidateSelectionKey {
    CandidateSelectionKey {
        url: candidate.url.clone(),
        chunk_text: candidate.chunk_text.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(in crate::vector::ops::commands::ask::context) struct ContextCandidateSelection {
    pub(in crate::vector::ops::commands::ask::context) candidate_index: usize,
    pub(in crate::vector::ops::commands::ask::context) key: CandidateSelectionKey,
    pub(in crate::vector::ops::commands::ask::context) url: String,
    pub(in crate::vector::ops::commands::ask::context) decisions: Vec<AskExplainSelectionDecision>,
    pub(in crate::vector::ops::commands::ask::context) metadata: CandidateSelectionMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::vector::ops::commands::ask::context) struct CandidateSelectionMetadata {
    pub(in crate::vector::ops::commands::ask::context) planned_full_doc_rank: Option<usize>,
    pub(in crate::vector::ops::commands::ask::context) selected_context_rank: Option<usize>,
    pub(in crate::vector::ops::commands::ask::context) insertion_mode:
        Option<AskExplainInsertionMode>,
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
    pub(crate) final_source_order: &'a [AskExplainContextSource],
}

pub(crate) fn build_context_selection_decisions(
    inputs: ContextSelectionInputs<'_>,
) -> Vec<ContextCandidateSelection> {
    let top_chunk_set = index_set(inputs.top_chunk_indices);
    let selected_top_chunk_set = index_set(inputs.selected_top_chunk_indices);
    let top_full_doc_set = index_set(inputs.top_full_doc_indices);
    let planned_full_doc_ranks = index_rank_map(inputs.top_full_doc_indices);
    let selected_context_ranks = selected_context_rank_map(inputs.final_source_order);
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
            let insertion_mode = insertion_mode_for_decisions(&decisions);
            let selected_context_rank = selected_context_ranks
                .get(&(candidate.url.clone(), insertion_mode))
                .copied();
            ContextCandidateSelection {
                candidate_index: idx,
                key: candidate_selection_key(candidate),
                url: candidate.url.clone(),
                metadata: CandidateSelectionMetadata {
                    planned_full_doc_rank: planned_full_doc_ranks.get(&idx).copied(),
                    selected_context_rank,
                    insertion_mode: Some(insertion_mode),
                },
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

fn insertion_mode_for_decisions(
    decisions: &[AskExplainSelectionDecision],
) -> AskExplainInsertionMode {
    if decisions
        .iter()
        .any(|decision| decision.kind == AskExplainSelectionDecisionKind::SelectedTopChunk)
    {
        AskExplainInsertionMode::TopChunk
    } else if decisions
        .iter()
        .any(|decision| decision.kind == AskExplainSelectionDecisionKind::InsertedFullDoc)
    {
        AskExplainInsertionMode::InsertedFullDoc
    } else if decisions
        .iter()
        .any(|decision| decision.kind == AskExplainSelectionDecisionKind::PlannedFullDoc)
    {
        AskExplainInsertionMode::PlannedFullDoc
    } else if decisions
        .iter()
        .any(|decision| decision.kind == AskExplainSelectionDecisionKind::SelectedSupplemental)
    {
        AskExplainInsertionMode::Supplemental
    } else {
        AskExplainInsertionMode::NotSelected
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

fn index_rank_map(indices: &[usize]) -> HashMap<usize, usize> {
    indices
        .iter()
        .copied()
        .enumerate()
        .map(|(rank, idx)| (idx, rank + 1))
        .collect()
}

fn selected_context_rank_map(
    sources: &[AskExplainContextSource],
) -> HashMap<(String, AskExplainInsertionMode), usize> {
    let mut ranks = HashMap::new();
    for (idx, source) in sources.iter().enumerate() {
        if let Some(mode) = insertion_mode_for_context_tier(&source.tier) {
            ranks.entry((source.url.clone(), mode)).or_insert(idx + 1);
        }
    }
    ranks
}

fn insertion_mode_for_context_tier(tier: &str) -> Option<AskExplainInsertionMode> {
    match tier {
        "top_chunk" => Some(AskExplainInsertionMode::TopChunk),
        "full_doc" => Some(AskExplainInsertionMode::InsertedFullDoc),
        "supplemental" => Some(AskExplainInsertionMode::Supplemental),
        _ => None,
    }
}

#[cfg(test)]
#[path = "trace/tests.rs"]
mod tests;

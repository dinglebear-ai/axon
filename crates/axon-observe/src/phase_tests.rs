use super::*;

#[test]
fn registry_covers_every_canonical_phase_in_contract_order() {
    let expected = [
        PipelinePhase::Queued,
        PipelinePhase::Requested,
        PipelinePhase::Resolving,
        PipelinePhase::Routing,
        PipelinePhase::Authorizing,
        PipelinePhase::Planning,
        PipelinePhase::Leasing,
        PipelinePhase::Discovering,
        PipelinePhase::Diffing,
        PipelinePhase::Fetching,
        PipelinePhase::Rendering,
        PipelinePhase::Enriching,
        PipelinePhase::Normalizing,
        PipelinePhase::Parsing,
        PipelinePhase::Graphing,
        PipelinePhase::Preparing,
        PipelinePhase::Batching,
        PipelinePhase::Embedding,
        PipelinePhase::Vectorizing,
        PipelinePhase::Upserting,
        PipelinePhase::Retrieving,
        PipelinePhase::Synthesizing,
        PipelinePhase::Evaluating,
        PipelinePhase::Publishing,
        PipelinePhase::Cleaning,
        PipelinePhase::Complete,
        PipelinePhase::Canceled,
    ];

    assert_eq!(PHASE_REGISTRY.len(), expected.len());
    for (entry, phase) in PHASE_REGISTRY.iter().zip(expected.iter()) {
        assert_eq!(entry.phase, *phase);
        assert!(!entry.applies_to.is_empty());
        assert!(!entry.meaning.is_empty());
    }
}

#[test]
fn describe_finds_every_registry_entry() {
    for entry in PHASE_REGISTRY {
        let found = describe(entry.phase).expect("every phase is registered");
        assert_eq!(found.meaning, entry.meaning);
        assert_eq!(found.applies_to, entry.applies_to);
    }
}

#[test]
fn label_matches_serde_snake_case_wire_form() {
    assert_eq!(label(PipelinePhase::Embedding), "embedding");
    assert_eq!(label(PipelinePhase::Complete), "complete");
    assert_eq!(label(PipelinePhase::Canceled), "canceled");
}

#[test]
fn meaning_and_applies_to_read_through_registry() {
    assert_eq!(
        meaning(PipelinePhase::Fetching),
        "network/local/package fetch"
    );
    assert_eq!(
        applies_to(PipelinePhase::Fetching),
        "source/research/summarize"
    );
}

#[test]
fn only_complete_and_canceled_are_terminal() {
    assert!(is_terminal(PipelinePhase::Complete));
    assert!(is_terminal(PipelinePhase::Canceled));
    assert!(!is_terminal(PipelinePhase::Embedding));
    assert!(!is_terminal(PipelinePhase::Queued));
}

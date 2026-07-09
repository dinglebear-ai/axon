use super::*;

/// `docs/pipeline-unification/plans/2026-07-08-rest-memory-surface.md` Task 3:
/// `import`/`export` must be listed alongside every other memory subaction.
#[test]
fn cli_memory_has_import_and_export_subcommands() {
    let subcommands = memory_subcommand_names();
    assert!(subcommands.contains(&"import"));
    assert!(subcommands.contains(&"export"));
}

#[test]
fn parses_minimal_remember_request() {
    let req = request_from_positionals(&[
        "remember".to_string(),
        "Memory content lives in Qdrant.".to_string(),
    ])
    .expect("request");

    assert!(matches!(req.subaction, Some(MemorySubaction::Remember)));
    assert_eq!(req.body.as_deref(), Some("Memory content lives in Qdrant."));
}

#[test]
fn parses_link_request() {
    let req = request_from_positionals(&[
        "link".to_string(),
        "source".to_string(),
        "target".to_string(),
        "--type".to_string(),
        "supersedes".to_string(),
    ])
    .expect("request");

    assert!(matches!(req.subaction, Some(MemorySubaction::Link)));
    assert_eq!(req.source_id.as_deref(), Some("source"));
    assert_eq!(req.target_id.as_deref(), Some("target"));
    assert!(matches!(
        req.edge_type,
        Some(axon_mcp::schema::MemoryEdgeType::Supersedes)
    ));
}

#[test]
fn parses_list_request() {
    let req = request_from_positionals(&[
        "list".to_string(),
        "--project".to_string(),
        "axon".to_string(),
        "--repo".to_string(),
        "jmagar/axon".to_string(),
        "--file".to_string(),
        "src/services/memory.rs".to_string(),
        "--type".to_string(),
        "decision".to_string(),
        "--status".to_string(),
        "superseded".to_string(),
        "--limit".to_string(),
        "20".to_string(),
    ])
    .expect("request");

    assert!(matches!(req.subaction, Some(MemorySubaction::List)));
    assert_eq!(req.project.as_deref(), Some("axon"));
    assert_eq!(req.repo.as_deref(), Some("jmagar/axon"));
    assert_eq!(req.file.as_deref(), Some("src/services/memory.rs"));
    assert!(matches!(req.memory_type, Some(MemoryNodeType::Decision)));
    assert_eq!(req.status.as_deref(), Some("superseded"));
    assert_eq!(req.limit, Some(20));
}

#[test]
fn parses_supersede_request() {
    let req = request_from_positionals(&[
        "supersede".to_string(),
        "replacement".to_string(),
        "old".to_string(),
    ])
    .expect("request");

    assert!(matches!(req.subaction, Some(MemorySubaction::Supersede)));
    assert_eq!(req.source_id.as_deref(), Some("replacement"));
    assert_eq!(req.target_id.as_deref(), Some("old"));
}

#[test]
fn parses_context_request() {
    let req = request_from_positionals(&[
        "context".to_string(),
        "--project".to_string(),
        "axon".to_string(),
        "--query".to_string(),
        "memory storage".to_string(),
        "--token-budget".to_string(),
        "2000".to_string(),
    ])
    .expect("request");

    assert!(matches!(req.subaction, Some(MemorySubaction::Context)));
    assert_eq!(req.project.as_deref(), Some("axon"));
    assert_eq!(req.query.as_deref(), Some("memory storage"));
    assert_eq!(req.token_budget, Some(2000));
}

#[test]
fn parses_reinforce_request() {
    let req = request_from_positionals(&[
        "reinforce".to_string(),
        "mem_1".to_string(),
        "--amount".to_string(),
        "0.3".to_string(),
    ])
    .expect("request");

    assert!(matches!(req.subaction, Some(MemorySubaction::Reinforce)));
    assert_eq!(req.id.as_deref(), Some("mem_1"));
    assert_eq!(req.amount, Some(0.3));
}

#[test]
fn parses_contradict_request() {
    let req = request_from_positionals(&[
        "contradict".to_string(),
        "mem_a".to_string(),
        "mem_b".to_string(),
        "--reason".to_string(),
        "conflict".to_string(),
    ])
    .expect("request");

    assert!(matches!(req.subaction, Some(MemorySubaction::Contradict)));
    assert_eq!(req.source_id.as_deref(), Some("mem_a"));
    assert_eq!(req.target_id.as_deref(), Some("mem_b"));
    assert_eq!(req.reason.as_deref(), Some("conflict"));
}

#[test]
fn parses_pin_and_unpin_requests() {
    let pin =
        request_from_positionals(&["pin".to_string(), "mem_1".to_string()]).expect("pin request");
    assert!(matches!(pin.subaction, Some(MemorySubaction::Pin)));
    assert_eq!(pin.pinned, Some(true));

    let unpin = request_from_positionals(&[
        "pin".to_string(),
        "mem_1".to_string(),
        "--unpin".to_string(),
    ])
    .expect("unpin request");
    assert_eq!(unpin.pinned, Some(false));
}

#[test]
fn parses_archive_and_forget_requests() {
    let archive = request_from_positionals(&[
        "archive".to_string(),
        "mem_1".to_string(),
        "--reason".to_string(),
        "stale".to_string(),
    ])
    .expect("archive request");
    assert!(matches!(archive.subaction, Some(MemorySubaction::Archive)));
    assert_eq!(archive.reason.as_deref(), Some("stale"));

    let forget = request_from_positionals(&["forget".to_string(), "mem_1".to_string()])
        .expect("forget request");
    assert!(matches!(forget.subaction, Some(MemorySubaction::Forget)));
    assert_eq!(forget.id.as_deref(), Some("mem_1"));
}

#[test]
fn parses_review_request() {
    let req = request_from_positionals(&[
        "review".to_string(),
        "--type".to_string(),
        "bug".to_string(),
        "--limit".to_string(),
        "5".to_string(),
    ])
    .expect("request");

    assert!(matches!(req.subaction, Some(MemorySubaction::Review)));
    assert!(matches!(req.memory_type, Some(MemoryNodeType::Bug)));
    assert_eq!(req.limit, Some(5));
}

#[test]
fn parses_compact_request() {
    let req = request_from_positionals(&[
        "compact".to_string(),
        "mem_a".to_string(),
        "mem_b".to_string(),
        "--strategy".to_string(),
        "concatenate".to_string(),
        "--archive-sources".to_string(),
    ])
    .expect("request");

    assert!(matches!(req.subaction, Some(MemorySubaction::Compact)));
    assert_eq!(
        req.memory_ids.as_deref(),
        Some(&["mem_a".to_string(), "mem_b".to_string()][..])
    );
    assert_eq!(req.strategy.as_deref(), Some("concatenate"));
    assert_eq!(req.archive_sources, Some(true));
}

#[test]
fn compact_requires_at_least_two_memory_ids() {
    let err = request_from_positionals(&["compact".to_string(), "mem_a".to_string()])
        .expect_err("should reject a single memory id");
    assert!(err.to_string().contains("at least 2"));
}

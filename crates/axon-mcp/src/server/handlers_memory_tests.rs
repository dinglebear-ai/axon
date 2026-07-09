use super::*;

/// `docs/pipeline-unification/plans/2026-07-08-rest-memory-surface.md` Task 3:
/// the MCP `memory` action's subaction registry must include `import`/
/// `export` alongside every other lifecycle subaction.
fn memory_subactions() -> Vec<&'static str> {
    [
        MemorySubaction::Remember,
        MemorySubaction::List,
        MemorySubaction::Search,
        MemorySubaction::Show,
        MemorySubaction::Link,
        MemorySubaction::Supersede,
        MemorySubaction::Context,
        MemorySubaction::Reinforce,
        MemorySubaction::Contradict,
        MemorySubaction::Pin,
        MemorySubaction::Archive,
        MemorySubaction::Forget,
        MemorySubaction::Review,
        MemorySubaction::Compact,
        MemorySubaction::Import,
        MemorySubaction::Export,
    ]
    .into_iter()
    .map(memory_subaction_label)
    .collect()
}

#[test]
fn mcp_memory_registry_contains_import_and_export() {
    let actions = memory_subactions();
    assert!(actions.contains(&"import"));
    assert!(actions.contains(&"export"));
}

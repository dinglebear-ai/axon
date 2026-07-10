use super::*;

/// Builds a `SourceScopeCapability` with a distinct, recognizable value in
/// every one of the 17 contract-shape fields (`required` is matrix-internal
/// bookkeeping, not itself a contract field — see the struct doc comment —
/// so it isn't part of the count but is still covered below) and asserts
/// each field round-trips through the `scope_capability()` const
/// constructor unchanged.
#[test]
fn scope_capability_round_trips_all_contract_fields() {
    let cap = scope_capability(
        SourceScope::Repo,
        true,
        "notes-value",
        true,
        false,
        true,
        false,
        true,
        false,
        true,
        false,
        true,
        "output-item-kind-value",
        "option-schema-value",
        "chunking-hints-value",
        &["required-fact-a", "required-fact-b"],
        &["optional-fact-a"],
        &["degraded-mode-a"],
    );

    assert_eq!(cap.scope, SourceScope::Repo);
    assert!(cap.required);
    assert_eq!(cap.notes, "notes-value");
    assert!(cap.embeds_by_default);
    assert!(!cap.watch_supported);
    assert!(cap.refresh_supported);
    assert!(!cap.requires_credentials);
    assert!(cap.may_access_local_paths);
    assert!(!cap.may_perform_network_fetches);
    assert!(cap.may_call_render_provider);
    assert!(!cap.may_execute_tools);
    assert!(cap.accepts_uploads);
    assert_eq!(cap.output_item_kind, "output-item-kind-value");
    assert_eq!(cap.option_schema, "option-schema-value");
    assert_eq!(cap.chunking_hints, "chunking-hints-value");
    assert_eq!(
        cap.required_graph_fact_kinds,
        &["required-fact-a", "required-fact-b"]
    );
    assert_eq!(cap.optional_graph_fact_kinds, &["optional-fact-a"]);
    assert_eq!(cap.degraded_modes, &["degraded-mode-a"]);
}

/// Flip every boolean the opposite way from the case above so a field that
/// silently aliased another (e.g. two params swapped in the constructor
/// call) would show up as a mismatch in at least one of the two tests.
#[test]
fn scope_capability_round_trips_with_inverted_booleans() {
    let cap = scope_capability(
        SourceScope::Page,
        false,
        "other-notes",
        false,
        true,
        false,
        true,
        false,
        true,
        false,
        true,
        false,
        "other-output-item-kind",
        "other-option-schema",
        "other-chunking-hints",
        &[],
        &[],
        &[],
    );

    assert_eq!(cap.scope, SourceScope::Page);
    assert!(!cap.required);
    assert_eq!(cap.notes, "other-notes");
    assert!(!cap.embeds_by_default);
    assert!(cap.watch_supported);
    assert!(!cap.refresh_supported);
    assert!(cap.requires_credentials);
    assert!(!cap.may_access_local_paths);
    assert!(cap.may_perform_network_fetches);
    assert!(!cap.may_call_render_provider);
    assert!(cap.may_execute_tools);
    assert!(!cap.accepts_uploads);
    assert_eq!(cap.output_item_kind, "other-output-item-kind");
    assert_eq!(cap.option_schema, "other-option-schema");
    assert_eq!(cap.chunking_hints, "other-chunking-hints");
    assert!(cap.required_graph_fact_kinds.is_empty());
    assert!(cap.optional_graph_fact_kinds.is_empty());
    assert!(cap.degraded_modes.is_empty());
}

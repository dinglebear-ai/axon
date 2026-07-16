use super::*;

/// Every action this generator treats as live must be accepted (non-denied)
/// by the real, already-public dispatcher authz oracle
/// (`axon_mcp::server::required_scope_for`). This is the drift guard: if
/// `MCP_ACTION_SPECS` in `crates/axon-mcp/src/server/authz.rs` drops an
/// action without a matching edit to `LIVE_ACTIONS` here, this fails.
#[test]
fn every_live_action_is_accepted_by_the_real_dispatcher_authz() {
    for spec in LIVE_ACTIONS {
        let sample_subaction = match spec.subaction {
            SubactionKind::None => "",
            SubactionKind::TypedEnum => {
                let variants = typed_subaction_variants(spec.name);
                assert!(
                    !variants.is_empty(),
                    "action {} declares TypedEnum subactions but the real enum has no variants",
                    spec.name
                );
                Box::leak(variants[0].clone().into_boxed_str())
            }
            SubactionKind::InformalStrings(values) => {
                assert!(
                    !values.is_empty(),
                    "action {} declares InformalStrings subactions but the list is empty",
                    spec.name
                );
                values[0]
            }
        };
        let resolved = axon_mcp::server::required_scope_for(spec.name, sample_subaction);
        assert_ne!(
            resolved,
            Some("__deny__"),
            "action {:?} (subaction {:?}) is in this generator's LIVE_ACTIONS but the real \
             dispatcher denies it — MCP_ACTION_SPECS in \
             crates/axon-mcp/src/server/authz.rs must have changed; update LIVE_ACTIONS to match",
            spec.name,
            sample_subaction
        );
    }
}

/// Every action this generator knows is *not* live (removed / HTTP-only /
/// never contracted) must still be denied by the real dispatcher. Catches
/// the reverse drift: an action silently re-added to `MCP_ACTION_SPECS`
/// without this generator learning about it would otherwise stay invisible.
#[test]
fn every_known_non_live_action_is_denied_by_the_real_dispatcher_authz() {
    for name in KNOWN_NON_LIVE_ACTIONS {
        let resolved = axon_mcp::server::required_scope_for(name, "");
        assert_eq!(
            resolved,
            Some("__deny__"),
            "action {name:?} is listed as non-live in this generator (KNOWN_NON_LIVE_ACTIONS) \
             but the real dispatcher now accepts it — it must be promoted to LIVE_ACTIONS"
        );
    }
}

/// Bidirectional coverage: the union of `LIVE_ACTIONS` and
/// `KNOWN_NON_LIVE_ACTIONS` names must not overlap (an action cannot be both
/// live and known-non-live at once — that would mean this file is
/// internally inconsistent).
#[test]
fn live_and_non_live_action_name_sets_are_disjoint() {
    let live: std::collections::BTreeSet<&str> = live_action_names().into_iter().collect();
    for name in KNOWN_NON_LIVE_ACTIONS {
        assert!(
            !live.contains(name),
            "action {name:?} appears in both LIVE_ACTIONS and KNOWN_NON_LIVE_ACTIONS"
        );
    }
}

/// Every `LIVE_ACTIONS` entry's `request_dto` must resolve through
/// `request_schema_for` (panics otherwise) and every `TypedEnum` action's
/// subaction enum must resolve through `typed_subaction_variants`.
#[test]
fn every_live_action_request_and_subaction_schema_resolves() {
    for spec in LIVE_ACTIONS {
        let schema = request_schema_for(spec.request_dto);
        assert!(
            schema.get("type").is_some() || schema.get("$ref").is_some() || schema.is_object(),
            "resolved schema for {} looks malformed",
            spec.request_dto
        );
        if matches!(spec.subaction, SubactionKind::TypedEnum) {
            let variants = typed_subaction_variants(spec.name);
            assert!(
                !variants.is_empty(),
                "{} has no subaction variants",
                spec.name
            );
        }
    }
}

/// `deferred_actions` must contain every contract action absent from the
/// live registry, and must never contain a live action name.
#[test]
fn deferred_actions_covers_exactly_the_contract_minus_live_delta() {
    let live: std::collections::BTreeSet<&str> = live_action_names().into_iter().collect();
    let deferred = deferred_actions();
    let deferred_names: std::collections::BTreeSet<String> = deferred
        .iter()
        .map(|v| v["action"].as_str().unwrap().to_string())
        .collect();
    for name in CONTRACT_ACTIONS {
        if live.contains(name) {
            assert!(
                !deferred_names.contains(*name),
                "contract action {name:?} is live but also listed as deferred"
            );
        } else {
            assert!(
                deferred_names.contains(*name),
                "contract action {name:?} is absent from the live registry but missing from \
                 deferred_actions"
            );
        }
    }
}

#[test]
fn clean_break_system_actions_have_only_canonical_subactions() {
    let subactions = |action| {
        let spec = LIVE_ACTIONS
            .iter()
            .find(|spec| spec.name == action)
            .expect("live system action");
        match spec.subaction {
            SubactionKind::InformalStrings(values) => values,
            _ => panic!("{action} must use an explicit static subaction set"),
        }
    };

    assert_eq!(subactions("prune"), ["plan", "exec"]);
    assert_eq!(subactions("reset"), ["plan", "exec"]);
    assert_eq!(subactions("collections"), ["list", "get"]);
    assert_eq!(
        subactions("uploads"),
        ["list", "create", "get", "put_content", "complete", "abort"]
    );
    for removed in ["crawl", "embed", "ingest", "dedupe", "purge"] {
        assert!(!live_action_names().contains(&removed));
    }
}

#[test]
fn only_unimplemented_contract_actions_are_deferred() {
    let names = deferred_actions()
        .into_iter()
        .map(|value| value["action"].as_str().unwrap().to_string())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        names,
        ["artifacts", "chat"]
            .into_iter()
            .map(str::to_string)
            .collect()
    );
}

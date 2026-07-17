use super::*;

// ── prune_selector_from_body (pure selector-building logic shared by both
// prune_plan and prune_exec) ────────────────────────────────────────────────

#[test]
fn prune_selector_rejects_empty_target() {
    let err = prune_selector_from_body("   ", None).expect_err("empty target must fail");
    assert_eq!(err.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn prune_selector_bare_source_id_without_generation() {
    let selector = prune_selector_from_body("src_123", None).expect("bare source id must parse");
    match selector {
        PruneSelector::Source { source_id } => assert_eq!(source_id.0, "src_123"),
        other => panic!("expected Source selector, got {other:?}"),
    }
}

#[test]
fn prune_selector_source_id_with_generation() {
    let selector = prune_selector_from_body("src_123", Some("gen_1"))
        .expect("source id + generation must parse");
    match selector {
        PruneSelector::Generation {
            source_id,
            generation,
        } => {
            assert_eq!(source_id.0, "src_123");
            assert_eq!(generation.0, "gen_1");
        }
        other => panic!("expected Generation selector, got {other:?}"),
    }
}

#[test]
fn prune_selector_collection_prefix_parses_whole_collection() {
    let selector =
        prune_selector_from_body("collection:axon", None).expect("collection target must parse");
    match selector {
        PruneSelector::Collection { collection } => assert_eq!(collection, "axon"),
        other => panic!("expected Collection selector, got {other:?}"),
    }
}

#[test]
fn prune_selector_collection_prefix_rejects_empty_name() {
    let err =
        prune_selector_from_body("collection:", None).expect_err("empty collection name must fail");
    assert_eq!(err.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn prune_selector_collection_prefix_rejects_generation() {
    let err = prune_selector_from_body("collection:axon", Some("gen_1"))
        .expect_err("collection target with generation must fail");
    assert_eq!(err.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn prune_selector_trims_whitespace_around_generation() {
    let selector = prune_selector_from_body("src_123", Some("  gen_1  "))
        .expect("whitespace-padded generation must still parse");
    match selector {
        PruneSelector::Generation { generation, .. } => assert_eq!(generation.0, "gen_1"),
        other => panic!("expected Generation selector, got {other:?}"),
    }
}

// ── PruneAuthz derivation (mirrors what prune_exec does with a resolved
// AuthContext — asserted directly against axon_authz::scope_satisfies since
// building a full WebState/ServiceContext is out of scope for a unit test) ──

#[test]
fn admin_scope_present_grants_prune_authz() {
    let scopes = vec!["axon:admin".to_string()];
    let authz = PruneAuthz {
        is_admin: axon_authz::scope_satisfies(&scopes, axon_authz::AXON_ADMIN_SCOPE),
    };
    assert!(authz.is_admin);
}

#[test]
fn write_only_scope_does_not_grant_prune_authz() {
    // Per the auth contract, axon:write does NOT imply axon:admin.
    let scopes = vec!["axon:write".to_string()];
    let authz = PruneAuthz {
        is_admin: axon_authz::scope_satisfies(&scopes, axon_authz::AXON_ADMIN_SCOPE),
    };
    assert!(!authz.is_admin);
}

#[test]
fn reset_plan_is_always_dry_run() {
    let cfg = Config::default();
    let request = ResetPlanRequest {
        dry_run: false,
        ..Default::default()
    };
    let error = reset_plan_config(&cfg, &request).expect_err("execution is a separate route");
    assert_eq!(error.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn reset_plan_can_exclude_artifacts_from_default_store_set() {
    let cfg = Config::default();
    let request = ResetPlanRequest {
        include_artifacts: Some(false),
        ..Default::default()
    };
    let planned = reset_plan_config(&cfg, &request).expect("valid reset plan config");
    assert!(!planned.reset_stores.is_empty());
    assert!(
        !planned
            .reset_stores
            .iter()
            .any(|store| store == RESET_STORE_ARTIFACTS)
    );
    assert!(planned.reset_dry_run);
    assert!(!planned.yes);
}

#[test]
fn reset_plan_rejects_collection_drift() {
    let cfg = Config::default();
    let request = ResetPlanRequest {
        collection: Some("another_collection".to_string()),
        ..Default::default()
    };
    let error = reset_plan_config(&cfg, &request).expect_err("plan id must bind config");
    assert_eq!(error.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn reset_plan_never_turns_an_empty_filtered_selection_into_reset_all() {
    let cfg = Config::default();
    let request = ResetPlanRequest {
        stores: vec![RESET_STORE_ARTIFACTS.to_string()],
        include_artifacts: Some(false),
        ..Default::default()
    };
    let error = reset_plan_config(&cfg, &request).expect_err("empty selection must fail");
    assert_eq!(error.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn reset_authz_requires_admin_scope() {
    let scopes = vec!["axon:write".to_string()];
    let authz = ResetAuthz {
        is_admin: axon_authz::scope_satisfies(&scopes, axon_authz::AXON_ADMIN_SCOPE),
    };
    assert!(!authz.is_admin);
}

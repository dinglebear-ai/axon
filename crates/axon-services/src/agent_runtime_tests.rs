use super::client::LabbyContextReceipt;
use super::*;

fn context() -> LabbyContextReceipt {
    LabbyContextReceipt {
        execution_context_id: "ctx_opaque".into(),
        actor: "actor@example.com".into(),
        service: "axon".into(),
        loadout_id: "loadout-a".into(),
        loadout_revision: 7,
        expires_at_unix_ms: i64::MAX,
        catalog_generation: "catalog-1".into(),
        exact_execution: true,
        llm_invoked: false,
    }
}

#[test]
fn durable_turn_rejects_idempotency_mismatch() {
    let store = AgentTurnStore::memory().unwrap();
    let turn = store
        .create(
            "turn-a",
            "loadout-a",
            7,
            "hello",
            i64::MAX,
            "owner-a",
            "profile-a",
            8,
            "model-a",
            &context(),
        )
        .unwrap();
    turn.verify_create_replay("owner-a", "profile-a", "loadout-a", 7, "hello")
        .unwrap();
    assert!(
        turn.verify_create_replay("owner-a", "profile-a", "loadout-a", 8, "hello")
            .is_err()
    );
    assert!(
        turn.verify_create_replay("owner-a", "profile-a", "loadout-a", 7, "changed")
            .is_err()
    );
    assert!(
        turn.verify_create_replay("owner-b", "profile-a", "loadout-a", 7, "hello")
            .is_err()
    );
}

#[test]
fn proposal_and_receipt_preserve_attribution_and_correlation() {
    let store = AgentTurnStore::memory().unwrap();
    store
        .create(
            "turn-a",
            "loadout-a",
            7,
            "hello",
            i64::MAX,
            "owner-a",
            "profile-a",
            8,
            "model-a",
            &context(),
        )
        .unwrap();
    let proposal = AgentToolProposal {
        tool_call_id: "turn-a:1".into(),
        tool_id: "github::get_issue".into(),
        contract_hash: "sha256:contract".into(),
        arguments: serde_json::json!({"id": 1}),
        destructive: false,
    };
    store.set_proposal("turn-a", &proposal).unwrap();
    store
        .reserve_execution("turn-a", "turn-a:1", "axon-agent:turn-a:turn-a:1")
        .unwrap();
    store
        .record_receipt(
            "turn-a",
            &proposal,
            &LabbyExecutionReceipt {
                request_id: "axon-agent:turn-a:turn-a:1".into(),
                receipt_id: "receipt-a".into(),
                audit_id: "audit-a".into(),
                status: "succeeded".into(),
                tool_id: proposal.tool_id.clone(),
                contract_hash: proposal.contract_hash.clone(),
                loadout_id: "loadout-a".into(),
                loadout_revision: 7,
                actor: "actor@example.com".into(),
                service: "axon".into(),
                execution_context_id: "ctx_opaque".into(),
                idempotency_key: "axon-agent:turn-a:turn-a:1".into(),
                result: Some(serde_json::json!({"ok":true})),
                error_kind: None,
            },
        )
        .unwrap();
    store
        .complete_tool("turn-a", "turn-a:1", serde_json::json!({"ok":true}))
        .unwrap();
    let result = store.result("turn-a").unwrap();
    assert_eq!(result.correlation.actor, "actor@example.com");
    assert_eq!(result.correlation.receipt_ids, vec!["receipt-a"]);
    assert_eq!(result.correlation.audit_ids, vec!["audit-a"]);
    assert_eq!(result.correlation.tool_call_count, 1);
}

#[test]
fn model_contract_fails_closed() {
    assert!(parse_action("not json").is_err());
    assert!(parse_action(r#"{"type":"tool","tool_id":"x","arguments":{}}"#).is_err());
    assert!(matches!(
        parse_action(r#"{"type":"final","answer":"done"}"#).unwrap(),
        ModelAction::Final { .. }
    ));
}

#[test]
fn bounds_reject_unbounded_or_missing_delegation() {
    let options = AgentTurnOptions {
        delegation_token: String::new(),
        turn_id: None,
        approval_tokens: vec![],
        max_tool_calls: 8,
        timeout_ms: 1000,
    };
    assert!(validate_options(&options).is_err());
    let options = AgentTurnOptions {
        delegation_token: "dlg".into(),
        max_tool_calls: 33,
        ..options
    };
    assert!(validate_options(&options).is_err());
}

#[test]
fn events_are_ordered_and_replayable_after_cursor() {
    let store = AgentTurnStore::memory().unwrap();
    store
        .create(
            "turn-a",
            "loadout-a",
            7,
            "hello",
            i64::MAX,
            "owner-a",
            "profile-a",
            8,
            "model-a",
            &context(),
        )
        .unwrap();
    store
        .transition("turn-a", AgentTurnStatus::Proposing, None)
        .unwrap();
    store
        .transition("turn-a", AgentTurnStatus::Cancelled, Some("cancelled"))
        .unwrap();
    assert_eq!(store.events("turn-a", 0).unwrap().len(), 2);
    assert_eq!(store.events("turn-a", 1).unwrap().len(), 1);
}

#[test]
fn concurrent_resume_uses_versioned_lease() {
    let store = AgentTurnStore::memory().unwrap();
    let turn = store
        .create(
            "turn-a",
            "loadout-a",
            7,
            "hello",
            i64::MAX,
            "owner-a",
            "profile-a",
            8,
            "model-a",
            &context(),
        )
        .unwrap();
    assert_eq!(
        store
            .acquire_lease("turn-a", "owner-a", turn.version, 1)
            .unwrap(),
        turn.version + 1
    );
    assert!(
        store
            .acquire_lease("turn-a", "owner-a", turn.version, 1)
            .is_err()
    );
}

#[test]
fn cancellation_is_monotonic_and_owner_private() {
    let store = AgentTurnStore::memory().unwrap();
    store
        .create(
            "turn-a",
            "loadout-a",
            7,
            "hello",
            i64::MAX,
            "owner-a",
            "profile-a",
            8,
            "model-a",
            &context(),
        )
        .unwrap();
    assert!(store.load_owned("turn-a", "owner-b").is_err());
    store.request_cancel("turn-a", "owner-a").unwrap();
    store
        .transition("turn-a", AgentTurnStatus::Succeeded, Some("late"))
        .unwrap();
    let turn = store.load("turn-a").unwrap().unwrap();
    assert!(turn.cancel_requested);
    assert_ne!(turn.status, AgentTurnStatus::Succeeded);
}

#[test]
fn request_id_is_not_the_idempotency_key_and_swapped_receipts_fail() {
    let store = AgentTurnStore::memory().unwrap();
    store
        .create(
            "turn-a",
            "loadout-a",
            7,
            "hello",
            i64::MAX,
            "owner-a",
            "profile-a",
            8,
            "model-a",
            &context(),
        )
        .unwrap();
    let proposal = AgentToolProposal {
        tool_call_id: "call-a".into(),
        tool_id: "tool-a".into(),
        contract_hash: "hash-a".into(),
        arguments: serde_json::json!({}),
        destructive: false,
    };
    store.set_proposal("turn-a", &proposal).unwrap();
    store
        .reserve_execution("turn-a", "call-a", "idem-a")
        .unwrap();
    assert_eq!(
        store.execution_request_id("turn-a", "call-a").unwrap(),
        None
    );
    let receipt = LabbyExecutionReceipt {
        request_id: "request-a".into(),
        receipt_id: "receipt-a".into(),
        audit_id: "audit-a".into(),
        status: "succeeded".into(),
        tool_id: "wrong-tool".into(),
        contract_hash: "hash-a".into(),
        loadout_id: "loadout-a".into(),
        loadout_revision: 7,
        actor: "actor@example.com".into(),
        service: "axon".into(),
        execution_context_id: "ctx_opaque".into(),
        idempotency_key: "idem-a".into(),
        result: None,
        error_kind: None,
    };
    assert!(store.record_receipt("turn-a", &proposal, &receipt).is_err());
}

#[test]
fn persisted_budget_deadline_model_and_profile_are_immutable() {
    let store = AgentTurnStore::memory().unwrap();
    let turn = store
        .create(
            "turn-a",
            "loadout-a",
            7,
            "hello",
            1234,
            "owner-a",
            "profile-a",
            3,
            "model-a",
            &context(),
        )
        .unwrap();
    assert_eq!(
        (
            turn.deadline_at_ms,
            turn.max_tool_calls,
            turn.model.as_str(),
            turn.profile_id.as_str()
        ),
        (1234, 3, "model-a", "profile-a")
    );
    assert!(
        turn.verify_create_replay("owner-a", "profile-b", "loadout-a", 7, "hello")
            .is_err()
    );
}

#[test]
fn lifecycle_maintenance_recovers_leases_and_prunes_terminal_turns() {
    let store = AgentTurnStore::memory().unwrap();
    let active = store
        .create(
            "active",
            "loadout-a",
            7,
            "hello",
            100,
            "owner-a",
            "profile-a",
            8,
            "model-a",
            &context(),
        )
        .unwrap();
    store
        .acquire_lease("active", "owner-a", active.version, 1)
        .unwrap();
    store
        .create(
            "old",
            "loadout-a",
            7,
            "hello",
            1,
            "owner-a",
            "profile-a",
            8,
            "model-a",
            &context(),
        )
        .unwrap();
    store
        .transition("old", AgentTurnStatus::Failed, Some("failed"))
        .unwrap();
    store.maintain(40_000, 10_000).unwrap();
    assert_eq!(
        store.load("active").unwrap().unwrap().status,
        AgentTurnStatus::Interrupted
    );
    assert!(store.load("old").unwrap().is_none());
}

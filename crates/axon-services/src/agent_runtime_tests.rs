use super::client::LabbyContextReceipt;
use super::*;

fn context() -> LabbyContextReceipt {
    LabbyContextReceipt {
        execution_context_id: "ctx_opaque".into(),
        actor: "actor@example.com".into(),
        service: "axon".into(),
        loadout_id: "loadout-a".into(),
        loadout_revision: 7,
    }
}

#[test]
fn durable_turn_rejects_idempotency_mismatch() {
    let store = AgentTurnStore::memory().unwrap();
    let turn = store
        .create("turn-a", "loadout-a", 7, "hello", i64::MAX, &context())
        .unwrap();
    turn.verify_resume("loadout-a", 7, "hello").unwrap();
    assert!(turn.verify_resume("loadout-a", 8, "hello").is_err());
    assert!(turn.verify_resume("loadout-a", 7, "changed").is_err());
}

#[test]
fn proposal_and_receipt_preserve_attribution_and_correlation() {
    let store = AgentTurnStore::memory().unwrap();
    store
        .create("turn-a", "loadout-a", 7, "hello", i64::MAX, &context())
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
        .create("turn-a", "loadout-a", 7, "hello", i64::MAX, &context())
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

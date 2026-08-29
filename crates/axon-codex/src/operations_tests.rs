use super::*;
use serde_json::json;

fn intent(key: &str, value: &str) -> OperationIntent {
    OperationIntent {
        actor: "user:1".into(),
        scope: "codex:plugins:write".into(),
        method: "plugin/install".into(),
        target_home_identity: "dev:1:ino:2".into(),
        runtime_boot_id: 1,
        policy_version: "v1".into(),
        expected_revision: Some("r1".into()),
        idempotency_key: key.into(),
        redacted_request: json!({"plugin":value}),
    }
}

#[test]
fn idempotency_is_actor_and_digest_bound() {
    let store = OperationStore::open_memory().unwrap();
    let first = store.create(&intent("same", "a")).unwrap();
    assert_eq!(store.create(&intent("same", "a")).unwrap().id, first.id);
    assert!(
        store
            .create(&intent("same", "b"))
            .unwrap_err()
            .contains("different parameters")
    );

    let mut different_method = intent("same", "a");
    different_method.method = "plugin/uninstall".into();
    assert!(
        store
            .create(&different_method)
            .unwrap_err()
            .contains("different parameters")
    );

    let mut different_scope = intent("same", "a");
    different_scope.scope = "codex:config:write".into();
    assert!(
        store
            .create(&different_scope)
            .unwrap_err()
            .contains("different parameters")
    );
}

#[test]
fn idempotency_keys_are_nonblank_and_bounded_at_the_store_boundary() {
    let store = OperationStore::open_memory().unwrap();
    for key in ["", "   ", "\n\t"] {
        assert!(
            store
                .create(&intent(key, "a"))
                .unwrap_err()
                .contains("blank")
        );
    }

    store.create(&intent(&"a".repeat(256), "a")).unwrap();
    assert!(
        store
            .create(&intent(&"a".repeat(257), "a"))
            .unwrap_err()
            .contains("256 bytes")
    );
}

#[test]
fn idempotency_is_bound_to_control_home_and_policy() {
    let store = OperationStore::open_memory().unwrap();
    let first = intent("target-bound", "a");
    store.create(&first).unwrap();

    let mut other_home = first.clone();
    other_home.target_home_identity = "dev:9:ino:9".into();
    assert!(
        store
            .create(&other_home)
            .unwrap_err()
            .contains("control target")
    );

    let mut other_policy = first;
    other_policy.policy_version = "v2".into();
    assert!(
        store
            .create(&other_policy)
            .unwrap_err()
            .contains("control target")
    );
}

#[test]
fn approval_is_single_use_and_queue_head_is_revalidated() {
    let store = OperationStore::open_memory().unwrap();
    let operation = store.create(&intent("one", "a")).unwrap();
    let capability = store.approve(operation.id, "admin:1").unwrap();
    assert!(
        store
            .begin_execution(
                operation.id,
                &capability,
                "plugin/install",
                &json!({"plugin":"a"}),
                Some("stale"),
                "dev:1:ino:2",
                1,
                "v1"
            )
            .unwrap_err()
            .contains("stale")
    );
    let executing = store
        .begin_execution(
            operation.id,
            &capability,
            "plugin/install",
            &json!({"plugin":"a"}),
            Some("r1"),
            "dev:1:ino:2",
            1,
            "v1",
        )
        .unwrap();
    assert_eq!(executing.phase, OperationPhase::Executing);
    assert!(
        store
            .begin_execution(
                operation.id,
                &capability,
                "plugin/install",
                &json!({"plugin":"a"}),
                Some("r1"),
                "dev:1:ino:2",
                1,
                "v1"
            )
            .is_err()
    );
    store.reconcile(operation.id, "r2").unwrap();
    assert_eq!(
        store.get(operation.id).unwrap().unwrap().phase,
        OperationPhase::Reconciled
    );
}

#[test]
fn interrupted_execution_requires_recovery_not_blind_retry() {
    let store = OperationStore::open_memory().unwrap();
    let operation = store.create(&intent("recover", "a")).unwrap();
    let capability = store.approve(operation.id, "admin:1").unwrap();
    store
        .begin_execution(
            operation.id,
            &capability,
            "plugin/install",
            &json!({"plugin":"a"}),
            Some("r1"),
            "dev:1:ino:2",
            1,
            "v1",
        )
        .unwrap();
    assert_eq!(store.recover_interrupted().unwrap(), 1);
    assert_eq!(
        store.get(operation.id).unwrap().unwrap().phase,
        OperationPhase::RecoveryRequired
    );
    let recovered = store.get(operation.id).unwrap().unwrap();
    assert_eq!(recovered.redacted_request, json!({"plugin":"a"}));
    assert_eq!(recovered.expected_revision.as_deref(), Some("r1"));
    store
        .retain_recovery(operation.id, "intended effect is absent")
        .unwrap();
    let unresolved = store.get(operation.id).unwrap().unwrap();
    assert_eq!(unresolved.phase, OperationPhase::RecoveryRequired);
    assert_eq!(
        unresolved.recovery_state.as_deref(),
        Some("intended effect is absent")
    );
}

#[test]
fn exact_recovery_lookup_is_not_limited_by_the_unfinished_page() {
    let store = OperationStore::open_memory().unwrap();
    let oldest = store.create(&intent("oldest", "a")).unwrap();
    for index in 0..101 {
        store.create(&intent(&format!("new-{index}"), "a")).unwrap();
    }
    let recovery = store.get_for_recovery(oldest.id).unwrap().unwrap();
    assert_eq!(recovery.operation.id, oldest.id);
    assert_eq!(recovery.target_home_identity, "dev:1:ino:2");
    assert_eq!(recovery.runtime_boot_id, 1);
    assert_eq!(recovery.policy_version, "v1");
}

#[test]
fn response_evidence_and_non_replay_disposition_are_durable() {
    let store = OperationStore::open_memory().unwrap();
    let operation = store.create(&intent("one-shot", "a")).unwrap();
    let capability = store.approve(operation.id, "admin:1").unwrap();
    let executing = store
        .begin_execution(
            operation.id,
            &capability,
            "plugin/install",
            &json!({"plugin":"a"}),
            Some("r1"),
            "dev:1:ino:2",
            1,
            "v1",
        )
        .unwrap();
    assert!(executing.execution_attempt_id.is_some());
    let evidence = store
        .record_response_evidence(operation.id, &json!({"accepted":true}))
        .unwrap();
    assert_eq!(
        store
            .get_for_recovery(operation.id)
            .unwrap()
            .unwrap()
            .operation
            .response_evidence
            .as_deref(),
        Some(evidence.as_str())
    );
    store
        .resolve_without_replay(operation.id, false, "confirmed absent")
        .unwrap();
    let resolved = store.get(operation.id).unwrap().unwrap();
    assert_eq!(resolved.phase, OperationPhase::Failed);
    assert_eq!(
        resolved.recovery_state.as_deref(),
        Some("operator_disposition_without_replay:confirmed absent")
    );
}

#[test]
fn pending_and_approved_operations_can_be_cancelled_only_once() {
    let store = OperationStore::open_memory().unwrap();

    let pending = store.create(&intent("cancel-pending", "a")).unwrap();
    store.cancel(pending.id).unwrap();
    let cancelled = store.get(pending.id).unwrap().unwrap();
    assert_eq!(cancelled.phase, OperationPhase::Denied);
    assert_eq!(
        cancelled.recovery_state.as_deref(),
        Some("operator_cancelled")
    );
    assert!(
        store
            .cancel(pending.id)
            .unwrap_err()
            .contains("cannot be cancelled")
    );

    let approved = store.create(&intent("cancel-approved", "a")).unwrap();
    let capability = store.approve(approved.id, "admin:1").unwrap();
    store.cancel(approved.id).unwrap();
    assert_eq!(
        store.get(approved.id).unwrap().unwrap().phase,
        OperationPhase::Denied
    );
    assert!(
        store
            .begin_execution(
                approved.id,
                &capability,
                "plugin/install",
                &json!({"plugin":"a"}),
                Some("r1"),
                "dev:1:ino:2",
                1,
                "v1",
            )
            .unwrap_err()
            .contains("invalid or already consumed")
    );
}

#[test]
fn expired_approval_is_terminal_and_clears_its_capability() {
    let store = OperationStore::open_memory().unwrap();
    let operation = store.create(&intent("expired", "a")).unwrap();
    let capability = store.approve(operation.id, "admin:1").unwrap();
    store
        .connection
        .lock()
        .unwrap()
        .execute(
            "UPDATE codex_control_operations SET expires_at=unixepoch()-1 WHERE id=?1",
            [operation.id],
        )
        .unwrap();

    assert!(
        store
            .begin_execution(
                operation.id,
                &capability,
                "plugin/install",
                &json!({"plugin":"a"}),
                Some("r1"),
                "dev:1:ino:2",
                1,
                "v1",
            )
            .unwrap_err()
            .contains("expired")
    );
    assert_eq!(
        store.get(operation.id).unwrap().unwrap().phase,
        OperationPhase::Expired
    );
    assert!(store.approve(operation.id, "admin:2").is_err());
}

#[test]
fn invalid_phase_transitions_do_not_change_operation_state() {
    let store = OperationStore::open_memory().unwrap();
    let operation = store.create(&intent("invalid-transition", "a")).unwrap();

    assert!(store.reconcile(operation.id, "r2").is_err());
    assert!(store.fail_ambiguous(operation.id, "not executing").is_err());
    assert!(store.resolve_recovery(operation.id, "r2").is_err());
    assert!(
        store
            .retain_recovery(operation.id, "not recovering")
            .is_err()
    );
    assert!(
        store
            .resolve_without_replay(operation.id, true, "not recovering")
            .is_err()
    );
    assert_eq!(
        store.get(operation.id).unwrap().unwrap().phase,
        OperationPhase::Pending
    );

    store.approve(operation.id, "admin:1").unwrap();
    assert!(store.approve(operation.id, "admin:2").is_err());
    assert_eq!(
        store.get(operation.id).unwrap().unwrap().phase,
        OperationPhase::Approved
    );
}

#[test]
fn sqlite_reopen_preserves_approval_execution_and_reconciliation() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("codex-control.db");
    let (id, capability) = {
        let store = OperationStore::open(&database).unwrap();
        let operation = store.create(&intent("reopen", "a")).unwrap();
        let capability = store.approve(operation.id, "admin:1").unwrap();
        (operation.id, capability)
    };

    {
        let reopened = OperationStore::open(&database).unwrap();
        let approved = reopened.get(id).unwrap().unwrap();
        assert_eq!(approved.phase, OperationPhase::Approved);
        assert_eq!(approved.approver.as_deref(), Some("admin:1"));
        reopened
            .begin_execution(
                id,
                &capability,
                "plugin/install",
                &json!({"plugin":"a"}),
                Some("r1"),
                "dev:1:ino:2",
                1,
                "v1",
            )
            .unwrap();
    }

    {
        let reopened = OperationStore::open(&database).unwrap();
        let executing = reopened.get_for_recovery(id).unwrap().unwrap();
        assert_eq!(executing.operation.phase, OperationPhase::Executing);
        assert!(executing.operation.execution_attempt_id.is_some());
        reopened.reconcile(id, "r2").unwrap();
    }

    let reopened = OperationStore::open(&database).unwrap();
    let reconciled = reopened.get(id).unwrap().unwrap();
    assert_eq!(reconciled.phase, OperationPhase::Reconciled);
    assert_eq!(reconciled.post_state_revision.as_deref(), Some("r2"));
}

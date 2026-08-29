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
}

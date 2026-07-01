use super::*;

#[tokio::test]
async fn fake_ledger_acquires_conflicts_reclaims_and_releases_leases() {
    let ledger = FakeLedgerStore::new();

    let first = ledger
        .acquire_lease(lease_request("source:src_a:refresh", "owner-a"))
        .await
        .unwrap()
        .expect("first lease");
    let conflict = ledger
        .acquire_lease(lease_request("source:src_a:refresh", "owner-b"))
        .await
        .unwrap();
    assert_eq!(conflict, None);

    let wrong_owner_release = ledger
        .release_lease(first.lease_id.clone(), "owner-b".to_string())
        .await
        .unwrap_err();
    assert_eq!(
        wrong_owner_release.code.to_string(),
        "source.ledger.lease_owner_mismatch"
    );

    ledger
        .release_lease(first.lease_id.clone(), "owner-a".to_string())
        .await
        .unwrap();
    let expired = ledger
        .acquire_lease(lease_request_ttl("source:src_a:refresh", "owner-a", 0))
        .await
        .unwrap()
        .expect("zero ttl lease");
    let reclaimed = ledger
        .acquire_lease(lease_request("source:src_a:refresh", "owner-b"))
        .await
        .unwrap()
        .expect("expired lease should be reclaimable");
    assert_ne!(expired.lease_id, reclaimed.lease_id);
    assert_eq!(reclaimed.owner_id, "owner-b");

    let stale_heartbeat = ledger
        .heartbeat_lease(expired.lease_id.clone(), "owner-a".to_string(), 30)
        .await
        .unwrap();
    assert_eq!(stale_heartbeat, None);
    let stale_release = ledger
        .release_lease(expired.lease_id, "owner-a".to_string())
        .await
        .unwrap_err();
    assert_eq!(
        stale_release.code.to_string(),
        "source.ledger.lease_missing"
    );
    let owner_c_conflict = ledger
        .acquire_lease(lease_request("source:src_a:refresh", "owner-c"))
        .await
        .unwrap();
    assert_eq!(owner_c_conflict, None);

    ledger
        .release_lease(reclaimed.lease_id.clone(), "owner-b".to_string())
        .await
        .unwrap();
    let reacquired = ledger
        .acquire_lease(lease_request("source:src_a:refresh", "owner-a"))
        .await
        .unwrap();
    assert!(reacquired.is_some());
}

#[tokio::test]
async fn fake_ledger_same_owner_can_renew_lease() {
    let ledger = FakeLedgerStore::new();
    let first = ledger
        .acquire_lease(lease_request("source:src_a:refresh", "owner-a"))
        .await
        .unwrap()
        .expect("first lease");

    let renewed = ledger
        .acquire_lease(lease_request("source:src_a:refresh", "owner-a"))
        .await
        .unwrap()
        .expect("same owner renewal");

    assert_eq!(renewed.lease_id, first.lease_id);
    assert_eq!(renewed.acquired_at, first.acquired_at);
}

#[tokio::test]
async fn fake_ledger_heartbeat_rejects_expired_lease() {
    let ledger = FakeLedgerStore::new();
    let first = ledger
        .acquire_lease(lease_request_ttl("source:src_a:refresh", "owner-a", 0))
        .await
        .unwrap()
        .expect("first lease");

    let heartbeat = ledger
        .heartbeat_lease(first.lease_id, "owner-a".to_string(), 30)
        .await
        .unwrap();

    assert_eq!(heartbeat, None);
}

#[tokio::test]
async fn fake_ledger_heartbeat_rejects_wrong_owner() {
    let ledger = FakeLedgerStore::new();
    let first = ledger
        .acquire_lease(lease_request("source:src_a:refresh", "owner-a"))
        .await
        .unwrap()
        .expect("first lease");

    let heartbeat = ledger
        .heartbeat_lease(first.lease_id, "owner-b".to_string(), 30)
        .await
        .unwrap();

    assert_eq!(heartbeat, None);
}

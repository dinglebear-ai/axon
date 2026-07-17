use super::*;

#[test]
fn compatibility_scan_accumulates_every_page() {
    let mut scan = PayloadContractScan::default();
    scan.observe(&[serde_json::json!({
        "payload": { "payload_contract_version": TARGET_PAYLOAD_CONTRACT_VERSION }
    })]);
    scan.observe(&[serde_json::json!({ "payload": {} })]);

    let (versions, incompatible) = scan.finish();

    assert_eq!(
        versions,
        vec![
            "<missing>".to_string(),
            TARGET_PAYLOAD_CONTRACT_VERSION.to_string(),
        ]
    );
    assert!(
        incompatible,
        "a legacy point on a later page must fail compatibility"
    );
}

#[test]
fn compatibility_scan_accepts_all_current_pages() {
    let mut scan = PayloadContractScan::default();
    for _ in 0..3 {
        scan.observe(&[serde_json::json!({
            "payload": { "payload_contract_version": TARGET_PAYLOAD_CONTRACT_VERSION }
        })]);
    }

    let (versions, incompatible) = scan.finish();

    assert_eq!(versions, vec![TARGET_PAYLOAD_CONTRACT_VERSION.to_string()]);
    assert!(!incompatible);
}

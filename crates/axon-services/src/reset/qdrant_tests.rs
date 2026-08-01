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

#[test]
fn collection_dimension_reads_named_and_legacy_vectors() {
    assert_eq!(
        collection_dense_dimension(&serde_json::json!({
            "result": { "config": { "params": { "vectors": {
                "dense": { "size": 1024, "distance": "Cosine" }
            }}}}
        })),
        Some(1024)
    );
    assert_eq!(
        collection_dense_dimension(&serde_json::json!({
            "result": { "config": { "params": { "vectors": {
                "size": 768, "distance": "Cosine"
            }}}}
        })),
        Some(768)
    );
}

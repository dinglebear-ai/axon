use super::*;
use serde_json::json;

#[test]
fn map_domains_valid() {
    let payload = json!({
        "limit": 20,
        "offset": 5,
        "domains": [
            { "domain": "example.com", "vectors": 42 },
            { "domain": "docs.rs", "vectors": 100 }
        ]
    });
    let result = map_domains_payload(&payload).unwrap();
    assert_eq!(result.limit, 20);
    assert_eq!(result.offset, 5);
    assert_eq!(result.domains.len(), 2);
    assert_eq!(result.domains[0].domain, "example.com");
    assert_eq!(result.domains[0].vectors, 42);
    assert_eq!(result.domains[1].domain, "docs.rs");
    assert_eq!(result.domains[1].vectors, 100);
}

#[test]
fn map_domains_missing_domains_field() {
    let payload = json!({ "limit": 10, "offset": 0 });
    let err = map_domains_payload(&payload).unwrap_err();
    assert!(
        err.to_string().contains("domains"),
        "error must mention 'domains', got: {err}"
    );
}

#[test]
fn map_domains_entry_missing_domain_key() {
    let payload = json!({
        "limit": 10,
        "offset": 0,
        "domains": [{ "vectors": 5 }]
    });
    let err = map_domains_payload(&payload).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("domains[0]"),
        "error must reference domains[0], got: {msg}"
    );
}

#[test]
fn map_domains_empty() {
    let payload = json!({ "limit": 10, "offset": 0, "domains": [] });
    let result = map_domains_payload(&payload).unwrap();
    assert!(result.domains.is_empty());
    assert_eq!(result.limit, 10);
    assert_eq!(result.offset, 0);
}

#[test]
fn detailed_domains_uses_target_domain_and_canonical_uri_fields() {
    let payloads = vec![
        json!({
            "domain": "legacy.example",
            "url": "https://legacy.example/a",
            "web_domain": "example.com",
            "item_canonical_uri": "https://example.com/a",
        }),
        json!({
            "web_domain": "example.com",
            "source_canonical_uri": "https://example.com/source",
        }),
        json!({
            "web_domain": "docs.rs",
            "chunk_locator": { "canonical_uri": "https://docs.rs/crate" },
        }),
    ];

    let result = summarize_detailed_domains(&payloads);

    assert_eq!(result.domains.len(), 2);
    assert_eq!(result.domains[0].domain, "docs.rs");
    assert_eq!(result.domains[0].vectors, 1);
    assert_eq!(result.domains[0].urls, 1);
    assert_eq!(result.domains[1].domain, "example.com");
    assert_eq!(result.domains[1].vectors, 2);
    assert_eq!(result.domains[1].urls, 2);
}

#[test]
fn detailed_domains_does_not_count_legacy_url_without_target_uri() {
    let payloads = vec![json!({
        "web_domain": "example.com",
        "url": "https://legacy.example/a"
    })];

    let result = summarize_detailed_domains(&payloads);

    assert_eq!(result.domains.len(), 1);
    assert_eq!(result.domains[0].domain, "example.com");
    assert_eq!(result.domains[0].vectors, 1);
    assert_eq!(result.domains[0].urls, 0);
}

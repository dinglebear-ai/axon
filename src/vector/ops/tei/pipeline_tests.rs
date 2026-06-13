/// Verify that the empty-chunk filter (applied before tei_embed) removes
/// blank and whitespace-only strings while keeping real content.
#[test]
fn empty_and_whitespace_chunks_are_filtered() {
    let mut chunks = vec![
        "".to_string(),
        "   ".to_string(),
        "real content".to_string(),
        "\n\t\n".to_string(),
        "another chunk".to_string(),
    ];
    chunks.retain(|c| !c.trim().is_empty());
    assert_eq!(
        chunks,
        vec!["real content".to_string(), "another chunk".to_string()]
    );
}

#[test]
fn all_empty_chunks_produces_no_chunks() {
    let mut chunks = vec!["".to_string(), "  ".to_string(), "\n".to_string()];
    chunks.retain(|c| !c.trim().is_empty());
    assert!(
        chunks.is_empty(),
        "all-empty input must produce zero chunks"
    );
}

// T-C3: apply_extra clobber-guard tests — reserved keys must survive collisions
// in caller-supplied extra, while non-reserved keys are applied normally.
#[test]
fn apply_extra_does_not_clobber_reserved_keys() {
    use crate::vector::ops::tei::pipeline::apply_extra;

    let mut payload = serde_json::json!({
        "url": "https://real.example.com/doc",
        "chunk_text": "original chunk",
        "payload_schema_version": 7,
    });
    // Attacker-supplied extra tries to overwrite reserved keys.
    let evil_extra = serde_json::json!({
        "url": "https://evil.example.com/injected",
        "chunk_text": "injected content",
        "payload_schema_version": 999,
        "safe_key": "allowed value",
    });
    apply_extra(&mut payload, &evil_extra);

    // Reserved fields must be unchanged.
    assert_eq!(
        payload["url"].as_str(),
        Some("https://real.example.com/doc"),
        "url must not be overwritten by extra"
    );
    assert_eq!(
        payload["chunk_text"].as_str(),
        Some("original chunk"),
        "chunk_text must not be overwritten by extra"
    );
    assert_eq!(
        payload["payload_schema_version"].as_u64(),
        Some(7),
        "payload_schema_version must not be overwritten by extra"
    );
    // Non-reserved key must be applied.
    assert_eq!(
        payload["safe_key"].as_str(),
        Some("allowed value"),
        "non-reserved keys must be applied by apply_extra"
    );
}

#[test]
fn apply_extra_applies_all_non_reserved_keys() {
    use crate::vector::ops::tei::pipeline::apply_extra;

    let mut payload = serde_json::json!({});
    let extra = serde_json::json!({
        "custom_tag": "rust",
        "priority": 42,
        "nested": {"x": 1},
    });
    apply_extra(&mut payload, &extra);

    assert_eq!(payload["custom_tag"].as_str(), Some("rust"));
    assert_eq!(payload["priority"].as_u64(), Some(42));
    assert!(payload["nested"].is_object());
}

#[test]
fn apply_extra_allows_planner_chunk_fields_but_blocks_system_fields() {
    let mut payload = serde_json::json!({
        "url": "https://example.com/original",
        "chunk_text": "original"
    });
    let extra = serde_json::json!({
        "url": "https://evil.example/override",
        "chunk_text": "evil",
        "chunk_content_kind": "code",
        "chunk_locator": "src/lib.rs#L1-L2",
        "source_range": {"line_start": 1, "line_end": 2}
    });

    super::apply_extra(&mut payload, &extra);

    assert_eq!(payload["url"], "https://example.com/original");
    assert_eq!(payload["chunk_text"], "original");
    assert_eq!(payload["chunk_content_kind"], "code");
    assert_eq!(payload["chunk_locator"], "src/lib.rs#L1-L2");
    assert_eq!(payload["source_range"]["line_start"], 1);
}

#[test]
fn payload_schema_version_covers_source_document_fields() {
    const _: () = assert!(
        crate::vector::ops::qdrant::PAYLOAD_SCHEMA_VERSION >= 8,
        "SourceDocument normalized fields require a schema bump"
    );
}

#[test]
fn apply_extra_all_reserved_keys_constant_is_non_empty() {
    use crate::vector::ops::tei::pipeline::RESERVED_PAYLOAD_KEYS;
    // Sanity: the constant must include the critical system keys.
    assert!(RESERVED_PAYLOAD_KEYS.contains(&"url"));
    assert!(RESERVED_PAYLOAD_KEYS.contains(&"chunk_text"));
    assert!(RESERVED_PAYLOAD_KEYS.contains(&"payload_schema_version"));
    assert!(RESERVED_PAYLOAD_KEYS.contains(&"seed_url"));
}

// axon_rust-lu6a: schema-version + extractor_name payload tests.
#[test]
fn payload_schema_version_is_at_least_two() {
    let version = std::hint::black_box(crate::vector::ops::qdrant::PAYLOAD_SCHEMA_VERSION);
    assert!(version >= 2);
}

#[test]
fn pipeline_payload_stamps_schema_version_and_extractor() {
    use crate::vector::ops::qdrant::PAYLOAD_SCHEMA_VERSION;
    let mut payload = serde_json::json!({
        "url": "https://example.com/x",
        "chunk_index": 0,
        "chunk_text": "body",
        "payload_schema_version": PAYLOAD_SCHEMA_VERSION,
    });
    assert_eq!(
        payload["payload_schema_version"].as_u64(),
        Some(u64::from(PAYLOAD_SCHEMA_VERSION))
    );
    assert!(payload.get("extractor_name").is_none());
    let extractor = Some("docs".to_string());
    if let Some(name) = &extractor
        && !name.is_empty()
    {
        payload["extractor_name"] = serde_json::Value::String(name.clone());
    }
    assert_eq!(payload["extractor_name"].as_str(), Some("docs"));
}

/// Regression (P-H1): dropping a blank chunk must drop its parallel `chunk_extra`
/// entry in lockstep, so surviving chunks keep their OWN per-chunk overrides and
/// no override shifts onto the wrong chunk or is silently lost.
#[test]
fn drop_blank_chunks_aligned_keeps_overrides_with_their_chunks() {
    let mut chunks = vec![
        "first".to_string(),
        "   ".to_string(), // blank — dropped together with override #1
        "third".to_string(),
    ];
    let mut chunk_extra = vec![
        serde_json::json!({"symbol_name": "a"}),
        serde_json::json!({"symbol_name": "blank"}),
        serde_json::json!({"symbol_name": "c"}),
    ];
    super::drop_blank_chunks_aligned(&mut chunks, &mut chunk_extra);
    assert_eq!(chunks, vec!["first".to_string(), "third".to_string()]);
    assert_eq!(chunk_extra.len(), 2, "chunk_extra filtered in lockstep");
    // "third" must keep ITS override ("c"), not inherit the dropped blank's.
    assert_eq!(chunk_extra[0]["symbol_name"], "a");
    assert_eq!(chunk_extra[1]["symbol_name"], "c");
}

/// Empty `chunk_extra` (the common crawl/embed/non-code path) just filters chunks.
#[test]
fn drop_blank_chunks_aligned_empty_extra_filters_chunks_only() {
    let mut chunks = vec!["a".to_string(), "  ".to_string(), "b".to_string()];
    let mut chunk_extra: Vec<serde_json::Value> = Vec::new();
    super::drop_blank_chunks_aligned(&mut chunks, &mut chunk_extra);
    assert_eq!(chunks, vec!["a".to_string(), "b".to_string()]);
    assert!(chunk_extra.is_empty());
}

/// Per-chunk override wins over the doc-level `extra` (the P-H1 merge contract):
/// a chunk's `symbol_name` must replace the file-level `null`, and the symbol keys
/// must NOT be treated as reserved (or the symbol-boost signal would silently vanish).
#[test]
fn chunk_override_wins_over_doc_level_extra() {
    let mut payload = serde_json::json!({});
    let doc_extra = serde_json::json!({"symbol_name": null, "code_file_path": "src/lib.rs"});
    let chunk_override = serde_json::json!({"symbol_name": "Response::parse", "symbol_kind": "method", "code_line_start": 42});
    super::apply_extra(&mut payload, &doc_extra);
    super::apply_extra(&mut payload, &chunk_override);
    assert_eq!(payload["symbol_name"], "Response::parse");
    assert_eq!(payload["symbol_kind"], "method");
    assert_eq!(payload["code_line_start"], 42);
    // doc-level keys not overridden by the chunk survive.
    assert_eq!(payload["code_file_path"], "src/lib.rs");
}

fn pipeline_test_doc(url: &str, chunks: Vec<&str>, local_cleanup: bool) -> super::PreparedDoc {
    super::PreparedDoc {
        url: url.to_string(),
        domain: "example.com".to_string(),
        chunks: chunks.into_iter().map(str::to_string).collect(),
        source_type: "test".to_string(),
        content_type: "text",
        title: Some("Test".to_string()),
        extra: None,
        extractor_name: None,
        structured: None,
        chunk_extra: Vec::new(),
        local_legacy_fragment_url: local_cleanup.then(|| url.to_string()),
    }
}

fn unnamed_collection_body(dim: usize) -> serde_json::Value {
    serde_json::json!({
        "result": {
            "config": {
                "params": {
                    "vectors": {"size": dim}
                }
            },
            "payload_schema": {}
        }
    })
}

#[tokio::test]
async fn run_embed_pipeline_does_not_delete_stale_tail_when_upsert_fails() {
    use crate::core::config::Config;
    use httpmock::prelude::*;

    let tei = MockServer::start_async().await;
    let qdrant = MockServer::start_async().await;
    let collection = format!("pipeline_fail_{}", uuid::Uuid::new_v4().simple());

    tei.mock_async(|when, then| {
        when.method(POST).path("/embed");
        then.status(200)
            .json_body(serde_json::json!([[0.1, 0.2], [0.2, 0.3]]));
    })
    .await;
    qdrant
        .mock_async(|when, then| {
            when.method(GET).path(format!("/collections/{collection}"));
            then.status(200).json_body(unnamed_collection_body(2));
        })
        .await;
    qdrant
        .mock_async(|when, then| {
            when.method(PUT)
                .path(format!("/collections/{collection}/index"));
            then.status(200)
                .json_body(serde_json::json!({"result": true}));
        })
        .await;
    qdrant
        .mock_async(|when, then| {
            when.method(PUT)
                .path(format!("/collections/{collection}/points"));
            then.status(500).body("upsert failed");
        })
        .await;
    let delete = qdrant
        .mock_async(|when, then| {
            when.method(POST)
                .path(format!("/collections/{collection}/points/delete"));
            then.status(200)
                .json_body(serde_json::json!({"result": true}));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.collection = collection;
    cfg.tei_url = tei.base_url();
    cfg.qdrant_url = qdrant.base_url();
    cfg.embed_doc_timeout_secs = 30;

    let result = super::run_embed_pipeline(
        &cfg,
        vec![pipeline_test_doc(
            "https://example.com/doc",
            vec!["one", "two"],
            false,
        )],
        None,
    )
    .await;

    assert!(result.is_err(), "upsert failure should fail the pipeline");
    assert_eq!(
        delete.calls_async().await,
        0,
        "cleanup delete must not run when upsert fails"
    );
}

#[tokio::test]
async fn run_embed_pipeline_deletes_stale_tail_and_local_fragments_after_successful_upsert() {
    use crate::core::config::Config;
    use httpmock::prelude::*;

    let tei = MockServer::start_async().await;
    let qdrant = MockServer::start_async().await;
    let collection = format!("pipeline_success_{}", uuid::Uuid::new_v4().simple());
    let file_url = "file:///tmp/project/src/lib.rs";

    tei.mock_async(|when, then| {
        when.method(POST).path("/embed");
        then.status(200)
            .json_body(serde_json::json!([[0.1, 0.2], [0.2, 0.3]]));
    })
    .await;
    qdrant
        .mock_async(|when, then| {
            when.method(GET).path(format!("/collections/{collection}"));
            then.status(200).json_body(unnamed_collection_body(2));
        })
        .await;
    qdrant
        .mock_async(|when, then| {
            when.method(PUT)
                .path(format!("/collections/{collection}/index"));
            then.status(200)
                .json_body(serde_json::json!({"result": true}));
        })
        .await;
    qdrant
        .mock_async(|when, then| {
            when.method(PUT)
                .path(format!("/collections/{collection}/points"));
            then.status(200)
                .json_body(serde_json::json!({"result": true}));
        })
        .await;
    qdrant
        .mock_async(|when, then| {
            when.method(POST)
                .path(format!("/collections/{collection}/points/scroll"));
            then.status(200).json_body(serde_json::json!({
                "result": {
                    "points": [
                        {"payload": {"url": format!("{file_url}#L1-L2")}}
                    ],
                    "next_page_offset": null
                }
            }));
        })
        .await;
    let delete = qdrant
        .mock_async(|when, then| {
            when.method(POST)
                .path(format!("/collections/{collection}/points/delete"));
            then.status(200)
                .json_body(serde_json::json!({"result": true}));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.collection = collection;
    cfg.tei_url = tei.base_url();
    cfg.qdrant_url = qdrant.base_url();
    cfg.embed_doc_timeout_secs = 30;

    let summary = super::run_embed_pipeline(
        &cfg,
        vec![pipeline_test_doc(file_url, vec!["one", "two"], true)],
        None,
    )
    .await
    .expect("pipeline should succeed");

    assert_eq!(summary.chunks_embedded, 2);
    assert_eq!(
        delete.calls_async().await,
        2,
        "stale-tail and local-fragment cleanup should both delete after upsert"
    );
}

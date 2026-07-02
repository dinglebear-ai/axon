use super::*;
use axon_core::config::Config;
use serde_json::json;

// ── map_query_results ─────────────────────────────────────────────────────

#[test]
fn map_query_results_passthrough_empty() {
    let result = map_query_results(vec![]).unwrap();
    assert!(result.results.is_empty());
}

#[test]
fn map_query_results_typed_nonempty() {
    let items = vec![
        json!({
            "rank": 1,
            "score": 0.9,
            "rerank_score": 1.1,
            "url": "https://a.com",
            "source": "a.com",
            "snippet": "alpha",
            "chunk_index": 2
        }),
        json!({
            "rank": 2,
            "score": 0.8,
            "rerank_score": 0.95,
            "url": "https://b.com",
            "source": "b.com",
            "snippet": "bravo",
            "chunk_index": null
        }),
    ];
    let result = map_query_results(items).unwrap();
    assert_eq!(result.results.len(), 2);
    assert_eq!(result.results[0].url, "https://a.com");
    assert_eq!(result.results[0].chunk_index, Some(2));
    assert_eq!(result.results[1].source, "b.com");
    assert_eq!(result.results[1].chunk_index, None);
}

#[test]
fn map_query_results_populates_code_fields_from_emitted_keys() {
    // Guards the query.rs emit -> QueryHit deserialize contract: every code key the
    // emitter writes must land in a QueryHit field. All code fields are
    // #[serde(default)] Option, so a key-name drift would silently null them
    // rather than error — this test catches that.
    let items = vec![json!({
        "rank": 1,
        "score": 0.9,
        "rerank_score": 1.0,
        "url": "https://github.com/x/y/blob/main/src/lib.rs#L1-L10",
        "source": "github.com",
        "snippet": "pub struct Buffer {}",
        "chunk_index": 0,
        "file_path": "src/lib.rs",
        "symbol": "Buffer",
        "kind": "struct",
        "start_line": 1,
        "end_line": 10,
        "file_type": "source",
        "language": "rust",
        "provider": "github",
        "content_kind": "file",
        "chunking_method": "tree_sitter",
        "symbol_extraction_status": "ok"
    })];
    let result = map_query_results(items).unwrap();
    let hit = &result.results[0];
    assert_eq!(hit.file_path.as_deref(), Some("src/lib.rs"));
    assert_eq!(hit.symbol.as_deref(), Some("Buffer"));
    assert_eq!(hit.kind.as_deref(), Some("struct"));
    assert_eq!(hit.start_line, Some(1));
    assert_eq!(hit.end_line, Some(10));
    assert_eq!(hit.file_type.as_deref(), Some("source"));
    assert_eq!(hit.language.as_deref(), Some("rust"));
    assert_eq!(hit.provider.as_deref(), Some("github"));
    assert_eq!(hit.content_kind.as_deref(), Some("file"));
    assert_eq!(hit.chunking_method.as_deref(), Some("tree_sitter"));
    assert_eq!(hit.symbol_extraction_status.as_deref(), Some("ok"));
}

#[test]
fn map_query_results_rejects_missing_required_fields() {
    let err = map_query_results(vec![json!({ "url": "https://a.com" })]).unwrap_err();
    assert!(
        err.to_string().contains("query result[0]"),
        "error should identify the bad result index, got: {err}"
    );
}

#[tokio::test]
async fn query_reports_typed_diagnostics_payload_without_ask_diagnostics() {
    use httpmock::Method::POST;
    use httpmock::MockServer;

    // TEI succeeds so query proceeds to vector mode probe.
    let tei = MockServer::start_async().await;
    tei.mock_async(|when, then| {
        when.method(POST).path("/embed");
        then.status(200)
            .json_body(json!([[0.1_f32, 0.2_f32, 0.3_f32, 0.4_f32]]));
    })
    .await;

    // Qdrant probe fails with 404, which should surface as structured diagnostics.
    let qdrant = MockServer::start_async().await;
    qdrant
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path_matches(regex::Regex::new("/collections/").unwrap());
            then.status(404);
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.tei_url = tei.base_url();
    cfg.qdrant_url = qdrant.base_url();
    cfg.collection = "diag_test_collection".to_string();
    cfg.ask_diagnostics = false;

    let err = query(
        &cfg,
        "diagnostics regression test query",
        Pagination {
            limit: 5,
            offset: 0,
        },
    )
    .await
    .expect_err("query should fail when collection is missing");

    let diag = diagnostics_from_error(err.as_ref())
        .expect("diagnostics payload should be attached without ask_diagnostics");
    assert_eq!(diag["stage"], "query_vector_search_dispatch");
    assert_eq!(diag["collection"], "diag_test_collection");
    assert_eq!(
        diag["qdrant_url"],
        reqwest::Url::parse(&qdrant.base_url()).unwrap().to_string()
    );
    assert_eq!(diag["query_len"], "diagnostics regression test query".len());
    assert_eq!(diag["mode"]["hybrid_search_enabled"], true);
    assert_eq!(diag["search_context"]["command"], "query");
    assert_eq!(diag["search_context"]["request_limit"], 80);
    assert_eq!(diag["search_context"]["sparse_query_empty"], false);
    assert!(diag["error"].as_str().unwrap_or("").contains("404"));
}

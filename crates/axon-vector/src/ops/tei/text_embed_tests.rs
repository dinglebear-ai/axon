use super::*;
use axon_core::config::Config;

// T-M2: embed_prepared_docs with empty input returns EmbedSummary{0,0,0} without
// contacting TEI or Qdrant — the empty guard fires before any I/O.
#[tokio::test]
async fn embed_prepared_docs_empty_input_returns_zero_summary() {
    let cfg = Config::test_default();
    let result = embed_prepared_docs(&cfg, vec![], None).await;
    assert!(
        result.is_ok(),
        "empty docs must not error: {:?}",
        result.err()
    );
    let summary = result.unwrap();
    assert_eq!(
        summary.docs_embedded, 0,
        "docs_embedded must be 0 for empty input"
    );
    assert_eq!(
        summary.docs_failed, 0,
        "docs_failed must be 0 for empty input"
    );
    assert_eq!(
        summary.chunks_embedded, 0,
        "chunks_embedded must be 0 for empty input"
    );
}

// T-M2: embed_path_native_with_progress returns Err when TEI_URL is not configured.
// No I/O is performed — the guard fires before any filesystem or network call.
#[tokio::test]
async fn embed_path_native_no_tei_url_returns_err() {
    let mut cfg = Config::test_default();
    cfg.tei_url = String::new();
    cfg.qdrant_url = "http://localhost:6333".to_string();

    let result = embed_path_native_with_progress(&cfg, "/tmp/nonexistent", None, None).await;
    assert!(result.is_err(), "missing TEI_URL must return Err");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("TEI_URL"),
        "error must mention TEI_URL, got: {msg}"
    );
}

// T-M2: embed_path_native_with_progress returns Err when QDRANT_URL is not configured.
#[tokio::test]
async fn embed_path_native_no_qdrant_url_returns_err() {
    let mut cfg = Config::test_default();
    cfg.tei_url = "http://localhost:8080".to_string();
    cfg.qdrant_url = String::new();

    let result = embed_path_native_with_progress(&cfg, "/tmp/nonexistent", None, None).await;
    assert!(result.is_err(), "missing QDRANT_URL must return Err");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("QDRANT_URL"),
        "error must mention QDRANT_URL, got: {msg}"
    );
}

use super::{vertical_doc_to_extract_run, write_extract_summary};
use axon_core::config::Config;
use axon_extract::ScrapedDoc;

#[tokio::test]
async fn extract_summary_redacts_secrets_before_writing() {
    let output_root = tempfile::tempdir().expect("output root");
    let cfg = Config {
        output_dir: output_root.path().to_path_buf(),
        output_path: None,
        ..Config::default()
    };

    let path = write_extract_summary(
        &cfg,
        &serde_json::json!({
            "runs": [{"error": "Authorization: Bearer abcdef0123456789abcdef"}],
        }),
    )
    .await
    .expect("write summary");

    let written = std::fs::read_to_string(&path).expect("read summary");
    assert!(!written.contains("abcdef0123456789abcdef"));
}

#[tokio::test]
async fn extract_summary_preserves_explicit_output_outside_output_dir() {
    let output_root = tempfile::tempdir().expect("output root");
    let explicit_root = tempfile::tempdir().expect("explicit root");
    let explicit = explicit_root.path().join("summary.json");
    let cfg = Config {
        output_dir: output_root.path().to_path_buf(),
        output_path: Some(explicit.clone()),
        ..Config::default()
    };

    let path = write_extract_summary(&cfg, &serde_json::json!({"ok": true}))
        .await
        .expect("write summary");

    assert_eq!(path, explicit);
    assert!(explicit.exists());
    assert!(!output_root.path().join("extract-summary.json").exists());
}

#[tokio::test]
async fn extract_summary_defaults_to_managed_output_dir() {
    let output_root = tempfile::tempdir().expect("output root");
    let cfg = Config {
        output_dir: output_root.path().to_path_buf(),
        output_path: None,
        ..Config::default()
    };

    let path = write_extract_summary(&cfg, &serde_json::json!({"ok": true}))
        .await
        .expect("write summary");

    assert_eq!(path, output_root.path().join("extract-summary.json"));
    assert!(path.exists());
}

#[test]
fn vertical_doc_becomes_extract_item() {
    let run = vertical_doc_to_extract_run(ScrapedDoc {
        url: "https://pypi.org/project/requests/".to_string(),
        markdown: "# requests\n\nPython HTTP library".to_string(),
        title: Some("requests".to_string()),
        extractor_name: "pypi",
        extractor_version: 3,
        structured: Some(serde_json::json!({"name": "requests"})),
        follow_crawl_urls: vec!["https://requests.readthedocs.io/".to_string()],
        extra: Some(serde_json::json!({"pkg_name": "requests"})),
    });

    assert_eq!(run.pages_visited, 1);
    assert_eq!(run.pages_with_data, 1);
    assert_eq!(run.results.len(), 1);
    assert_eq!(run.parser_hits.get("vertical:pypi"), Some(&1));
    assert_eq!(run.results[0]["extractor_name"], "pypi");
    assert_eq!(run.results[0]["extra"]["pkg_name"], "requests");
    assert_eq!(run.results[0]["structured"]["name"], "requests");
}

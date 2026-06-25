use super::write_extract_summary;
use axon_core::config::Config;

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

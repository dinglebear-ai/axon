use super::prepare_embed_docs;
use tempfile::TempDir;

#[tokio::test]
async fn prepare_embed_docs_uses_given_source_type() {
    let temp_dir = TempDir::new().expect("tempdir");
    let input_path = temp_dir.path().join("doc.md");
    tokio::fs::write(&input_path, "# Crawl doc\n\nhello there")
        .await
        .expect("write markdown fixture");

    let prepared = prepare_embed_docs(&input_path.to_string_lossy(), &[], Some("crawl"))
        .await
        .expect("prepare docs");

    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0].source_type, "crawl");
}

#[tokio::test]
async fn prepare_embed_docs_defaults_to_embed() {
    let temp_dir = TempDir::new().expect("tempdir");
    let input_path = temp_dir.path().join("doc.md");
    tokio::fs::write(&input_path, "# Embed doc\n\nthis is a test")
        .await
        .expect("write markdown fixture");

    let prepared = prepare_embed_docs(&input_path.to_string_lossy(), &[], None)
        .await
        .expect("prepare docs");

    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0].source_type, "embed");
}

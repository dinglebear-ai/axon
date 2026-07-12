use super::read_manifest_urls;
use std::collections::HashSet;

#[tokio::test]
async fn read_manifest_urls_returns_expected_set() {
    let fixture = tempfile::NamedTempFile::new().expect("create tempfile");
    tokio::fs::write(
        fixture.path(),
        "\nnot-json\n{\"url\":\"https://a.test\"}\n{\"url\":\"https://a.test\"}\n{\"other\":1}\n{\"url\":\"https://b.test\"}\n",
    )
    .await
    .expect("write fixture");

    let result = read_manifest_urls(fixture.path())
        .await
        .expect("parse manifest");
    let expected = HashSet::from(["https://a.test".to_string(), "https://b.test".to_string()]);
    assert_eq!(result, expected);
}

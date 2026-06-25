use super::*;

#[test]
fn classify_304() {
    assert_eq!(classify(304, None, None), Probe::NotModified);
}
#[test]
fn classify_200() {
    assert_eq!(
        classify(200, Some("\"a\"".into()), Some("d".into())),
        Probe::Modified {
            etag: Some("\"a\"".into()),
            last_modified: Some("d".into())
        }
    );
}
#[test]
fn classify_500_failed() {
    match classify(500, None, None) {
        Probe::Failed(m) => assert!(m.contains("500")),
        o => panic!("{o:?}"),
    }
}
#[test]
fn headers_present() {
    let h = conditional_headers(Some("\"a\""), Some("d"));
    assert!(h.iter().any(|(k, v)| k == "if-none-match" && v == "\"a\""));
    assert!(h.iter().any(|(k, v)| k == "if-modified-since" && v == "d"));
}
#[test]
fn headers_empty() {
    assert!(conditional_headers(None, None).is_empty());
}

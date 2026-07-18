use super::*;
use serde_json::json;

#[test]
fn nested_source_request_shape_extracts_url() {
    let req = json!({ "source_request": { "source": "https://www.reddit.com/r/rust/" } });
    let (url, source_type, target, urls) = request_target_fields(JobKind::Source, Some(&req));
    assert_eq!(url.as_deref(), Some("https://www.reddit.com/r/rust/"));
    assert_eq!(target.as_deref(), Some("https://www.reddit.com/r/rust/"));
    assert!(source_type.is_none());
    assert!(urls.is_none());
}

#[test]
fn flat_source_shape_extracts_url() {
    // Legacy `{"scope","source","source_kind"}` shape — the source lives at the
    // top level. Regression guard for the `axon status` `[REDACTED]` bug where
    // this fell through to `job.id` and the UUID tripped the secret redactor.
    let req = json!({
        "scope": "page",
        "source": "https://news.ycombinator.com/item?id=1",
        "source_kind": "web"
    });
    let (url, source_type, target, urls) = request_target_fields(JobKind::Source, Some(&req));
    assert_eq!(
        url.as_deref(),
        Some("https://news.ycombinator.com/item?id=1")
    );
    assert_eq!(
        target.as_deref(),
        Some("https://news.ycombinator.com/item?id=1")
    );
    assert!(source_type.is_none());
    assert!(urls.is_none());
}

#[test]
fn nested_shape_takes_precedence_over_flat_source() {
    // If both are somehow present, the canonical nested shape wins.
    let req = json!({
        "source": "https://flat.example/legacy",
        "source_request": { "source": "https://nested.example/canonical" }
    });
    let (url, _, target, _) = request_target_fields(JobKind::Source, Some(&req));
    assert_eq!(url.as_deref(), Some("https://nested.example/canonical"));
    assert_eq!(target.as_deref(), Some("https://nested.example/canonical"));
}

#[test]
fn no_source_anywhere_yields_none() {
    let req = json!({ "scope": "page", "source_kind": "web" });
    let (url, source_type, target, urls) = request_target_fields(JobKind::Source, Some(&req));
    assert!(url.is_none());
    assert!(source_type.is_none());
    assert!(target.is_none());
    assert!(urls.is_none());
}

#[test]
fn missing_request_json_yields_all_none() {
    let (url, source_type, target, urls) = request_target_fields(JobKind::Source, None);
    assert!(url.is_none() && source_type.is_none() && target.is_none() && urls.is_none());
}

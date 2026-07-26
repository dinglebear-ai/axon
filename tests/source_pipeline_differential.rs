//! Integration entrypoint for the cross-family source-route contract.

use axon_api::source::{SourceKind, SourceRequest, SourceScope};
use axon_services::source::routing::resolve_source_route;

#[test]
fn web_and_local_requests_enter_the_canonical_source_route() {
    let web = resolve_source_route(&SourceRequest::new("https://example.test/docs"))
        .expect("web request should route");
    let local_root = std::env::current_dir().expect("current directory");
    let local = resolve_source_route(&SourceRequest::local_path(
        local_root.to_string_lossy(),
        true,
    ))
    .expect("local request should route");

    assert_eq!(web.route.source.source_kind, SourceKind::Web);
    assert_eq!(local.route.source.source_kind, SourceKind::Local);
    assert_eq!(web.route.scope, SourceScope::Site);
    assert_eq!(local.route.scope, SourceScope::Directory);
    assert_eq!(web.route.adapter.version, local.route.adapter.version);
}

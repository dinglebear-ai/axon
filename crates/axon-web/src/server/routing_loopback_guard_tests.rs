use super::*;

#[test]
fn every_upload_mutation_requires_configured_auth_in_loopback_dev() {
    for (method, path) in [
        (Method::POST, "/v1/uploads"),
        (Method::PUT, "/v1/uploads/upl_test/content"),
        (Method::POST, "/v1/uploads/upl_test/complete"),
        (Method::PATCH, "/v1/uploads/upl_test"),
        (Method::DELETE, "/v1/uploads/upl_test"),
    ] {
        assert!(
            is_loopback_destructive_request(&method, path),
            "{method} {path}"
        );
    }
}

#[test]
fn upload_reads_remain_available_in_loopback_dev() {
    for path in ["/v1/uploads", "/v1/uploads/upl_test"] {
        assert!(
            !is_loopback_destructive_request(&Method::GET, path),
            "{path}"
        );
    }
}

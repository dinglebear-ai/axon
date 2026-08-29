use super::*;
fn request(product: BackendProduct, path: &str) -> BackendRequest {
    BackendRequest {
        profile_id: "profile-1".into(),
        product,
        request_id: "0123456789abcdef".into(),
        method: BackendMethod::Get,
        path: path.into(),
        body: None,
    }
}
#[test]
fn product_routes_fail_closed() {
    assert!(validate_request(&request(BackendProduct::Axon, "/v1/doctor")).is_ok());
    assert!(validate_request(&request(BackendProduct::Labby, "/v1/labby/profile")).is_ok());
    assert!(
        validate_request(&request(
            BackendProduct::Labby,
            "/v1/palette/descriptor?id=mcp%3Agithub%3A%3Asearch"
        ))
        .is_ok()
    );
    assert!(validate_request(&request(BackendProduct::Cortex, "/v1/cortex/profile")).is_ok());
    assert!(validate_request(&request(BackendProduct::Labby, "/v1/cortex/logs")).is_err());
    assert!(validate_request(&request(BackendProduct::Cortex, "/v1/palette/catalog")).is_err());
}
#[test]
fn rejects_origin_confusion_and_unsupported_major() {
    let mut p = BackendProfile {
        id: "p".into(),
        label: "P".into(),
        product: BackendProduct::Labby,
        origin: "https://labby.example/path".into(),
        credential_handle: None,
        pinned_server_id: None,
        accepted_api_major: 1,
    };
    assert!(validate_profile_origin(&p).is_err());
    p.origin = "https://labby.example".into();
    p.accepted_api_major = 2;
    assert!(validate_profile_origin(&p).is_err());
}
#[test]
fn request_ids_and_paths_are_bounded() {
    assert!(validate_request_id("short").is_err());
    assert!(validate_request_id("0123456789abcdef").is_ok());
    assert!(
        validate_request(&request(
            BackendProduct::Axon,
            "https://evil.example/v1/doctor"
        ))
        .is_err()
    );
    assert!(validate_request(&request(BackendProduct::Axon, "/v1/../admin")).is_err());
}

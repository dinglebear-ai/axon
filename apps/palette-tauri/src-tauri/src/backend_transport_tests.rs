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
    assert!(validate_request(&request(BackendProduct::Labby, "/v1/integration/identity")).is_ok());
    assert!(
        validate_request(&request(
            BackendProduct::Labby,
            "/v1/palette/descriptor?id=mcp%3Agithub%3A%3Asearch"
        ))
        .is_ok()
    );
    assert!(validate_request(&request(BackendProduct::Cortex, "/v1/integration/identity")).is_ok());
    assert!(validate_request(&request(BackendProduct::Cortex, "/api/search")).is_ok());
    assert!(validate_request(&request(BackendProduct::Labby, "/api/search")).is_err());
    assert!(validate_request(&request(BackendProduct::Cortex, "/v1/palette/catalog")).is_err());
    assert!(validate_request(&request(BackendProduct::Labby, "/v1/doctor")).is_err());
}
#[test]
fn rejects_origin_confusion_and_unsupported_major() {
    let mut p = BackendProfile {
        id: "p".into(),
        label: "P".into(),
        product: BackendProduct::Labby,
        origin: "https://labby.example/path".into(),
        credential_handle: None,
        credential_generation: None,
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

#[test]
fn stream_routes_are_real_cortex_routes_only() {
    assert!(validate_request(&request(BackendProduct::Cortex, "/api/streams/logs")).is_ok());
    assert!(validate_request(&request(BackendProduct::Labby, "/api/streams/logs")).is_err());
    assert!(
        validate_request(&request(
            BackendProduct::Cortex,
            "/v1/cortex/api/streams/logs"
        ))
        .is_err()
    );
}

#[test]
fn credentials_are_bound_to_profile_product_origin_server_and_generation() {
    let profile = BackendProfile {
        id: "labby-a".into(),
        label: "A".into(),
        product: BackendProduct::Labby,
        origin: "https://labby.example".into(),
        credential_handle: Some("handle-a".into()),
        credential_generation: Some("gen-2".into()),
        pinned_server_id: Some("server-a".into()),
        accepted_api_major: 1,
    };
    let credential = crate::backend_credentials::StoredBackendCredential {
        handle: "handle-a".into(),
        profile_id: "labby-a".into(),
        product: BackendProduct::Labby,
        origin: profile.origin.clone(),
        server_id: "server-a".into(),
        generation: "gen-2".into(),
        token: "secret".into(),
    };
    assert!(validate_credential_binding(&profile, &profile.origin, &credential).is_ok());
    for changed in [
        crate::backend_credentials::StoredBackendCredential {
            profile_id: "labby-b".into(),
            ..credential.clone()
        },
        crate::backend_credentials::StoredBackendCredential {
            product: BackendProduct::Cortex,
            ..credential.clone()
        },
        crate::backend_credentials::StoredBackendCredential {
            origin: "https://other.example".into(),
            ..credential.clone()
        },
        crate::backend_credentials::StoredBackendCredential {
            server_id: "server-b".into(),
            ..credential.clone()
        },
        crate::backend_credentials::StoredBackendCredential {
            generation: "gen-1".into(),
            ..credential.clone()
        },
    ] {
        assert!(validate_credential_binding(&profile, &profile.origin, &changed).is_err());
    }
}

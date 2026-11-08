use super::*;

#[test]
fn panel_identity_is_scoped_and_distinct_from_rest_bearer() {
    let auth = panel_auth_context();

    assert_eq!(auth.issuer, PANEL_AUTH_ISSUER);
    assert_eq!(auth.sub, "axon-panel");
    assert_eq!(auth.scopes, ["axon:read", "axon:write"]);
    assert!(!auth.scopes.iter().any(|scope| scope == "axon:admin"));
    assert!(!auth.scopes.iter().any(|scope| scope == "axon:local"));
}

#[test]
fn synthesis_account_status_is_in_admin_routes_and_contracts() {
    // The status includes a pending device-code challenge, not only metadata.
    let source = include_str!("routing.rs");
    let (ordinary, admin) = source.split_once("fn admin_routes(").unwrap();
    assert!(!ordinary.contains("\"/v1/synthesis/account\""));
    assert!(admin.contains("\"/v1/synthesis/account\""));
    let inventory = axon_services::types::rest_route_inventory();
    let entry = inventory
        .iter()
        .find(|r| r.path == "/v1/synthesis/account")
        .unwrap();
    assert_eq!(entry.auth, axon_services::types::RestRouteAuth::Admin);
    let schema = crate::schema_registry::rest_route_registry()
        .iter()
        .find(|r| r.path == "/v1/synthesis/account")
        .unwrap();
    assert_eq!(schema.required_scope, "admin");
}

#[tokio::test]
async fn synthesis_account_admin_gate_denies_read_and_write_but_allows_admin() {
    for (scope, expected) in [
        ("axon:read", StatusCode::FORBIDDEN),
        ("axon:write", StatusCode::FORBIDDEN),
        ("axon:admin", StatusCode::OK),
    ] {
        let mut identity = panel_auth_context();
        identity.scopes = vec![scope.into()];
        let app = Router::new()
            .route(
                "/v1/synthesis/account",
                axum::routing::get(|| async { "admin" }),
            )
            .route_layer(middleware::from_fn(require_admin_scope))
            .layer(Extension(identity));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
                .unwrap();
        });
        let response = reqwest::get(format!("http://{address}/v1/synthesis/account"))
            .await
            .unwrap();
        let _ = stop.send(());
        server.await.unwrap();
        assert_eq!(response.status(), expected, "scope {scope}");
    }
}

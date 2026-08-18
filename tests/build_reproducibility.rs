#[test]
fn swagger_ui_assets_are_vendored_for_network_free_rust_builds() {
    let manifest = include_str!("../crates/axon-web/Cargo.toml");
    let dependency = manifest
        .lines()
        .find(|line| line.trim_start().starts_with("utoipa-swagger-ui ="))
        .expect("axon-web must declare utoipa-swagger-ui");

    assert!(
        dependency.contains("\"vendored\""),
        "utoipa-swagger-ui must use vendored assets so Rust builds never download Swagger UI at compile time"
    );
}

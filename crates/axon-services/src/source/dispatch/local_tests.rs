use super::*;

#[tokio::test]
async fn local_plan_preserves_requested_byte_limits_including_zero() {
    let root = tempfile::tempdir().unwrap();
    let request = SourceRequest::local_path(root.path().to_string_lossy(), true);
    let route = crate::source::routing::resolve_source_route(&request)
        .unwrap()
        .route;
    for total in [0, 17, u64::MAX] {
        let limits = SourceLimits {
            max_bytes_per_item: Some(7),
            max_total_bytes: Some(total),
            ..Default::default()
        };
        let plan = local_source_plan(&request.source, &route, false, &limits, &[])
            .await
            .unwrap();
        assert_eq!(plan.request.limits, limits);
        assert_eq!(plan.limits.request, limits);
        assert_eq!(plan.limits.effective, limits);
    }
}

#[tokio::test]
async fn routed_local_discovery_keeps_configured_and_requested_exclusions() {
    let root = super::super::exclusion_tests::fixture();
    let mut request = SourceRequest::local_path(root.path().to_string_lossy(), true);
    request.options.values.insert(
        "exclude_paths".into(),
        serde_json::json!(["requested/", "configured/"]),
    );
    let route = crate::source::routing::resolve_source_route(&request)
        .unwrap()
        .route;
    let plan = local_source_plan(
        &request.source,
        &route,
        false,
        &SourceLimits::default(),
        &["configured/".into()],
    )
    .await
    .unwrap();
    super::super::exclusion_tests::assert_options(&plan);
    let manifest = axon_adapters::local::LocalSourceAdapter::new()
        .discover(&plan)
        .await
        .unwrap();
    let paths: Vec<_> = manifest
        .items
        .iter()
        .filter_map(|item| item.display_path.as_deref())
        .collect();
    assert_eq!(paths, ["kept/three.txt"]);
}

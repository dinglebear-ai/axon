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

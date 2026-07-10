use std::sync::Arc;

use super::*;

/// Compile-level assertion: `PruneServiceImpl` satisfies the trait and can
/// be built into a trait object.
#[allow(dead_code)]
fn assert_impl_is_prune_service(ctx: Arc<ServiceContext>) -> Arc<dyn PruneService> {
    Arc::new(PruneServiceImpl::new(ctx))
}

#[tokio::test]
async fn fake_prune_service_plan_is_dry_run_by_default() {
    let fake: Arc<dyn PruneService> = Arc::new(FakePruneService::new());
    let request = PruneRequest::dry_run(
        axon_api::source::PruneSelector::Source {
            source_id: axon_api::source::SourceId::new("source-1"),
        },
        "test",
    );
    let plan = fake.plan(request).await.expect("plan should succeed");
    assert!(!plan.destructive);
}

#[tokio::test]
async fn fake_prune_service_execute_requires_confirm() {
    let fake: Arc<dyn PruneService> = Arc::new(FakePruneService::new());
    let plan = fake
        .plan(PruneRequest::dry_run(
            axon_api::source::PruneSelector::Source {
                source_id: axon_api::source::SourceId::new("source-1"),
            },
            "test",
        ))
        .await
        .expect("plan should succeed");
    let request = PruneExecuteRequest {
        plan,
        confirm: false,
        reason: "test".to_string(),
    };
    assert!(fake.execute(request).await.is_err());
}

#[tokio::test]
async fn fake_prune_service_execute_succeeds_when_confirmed() {
    let fake: Arc<dyn PruneService> = Arc::new(FakePruneService::new());
    let plan = fake
        .plan(PruneRequest::dry_run(
            axon_api::source::PruneSelector::Source {
                source_id: axon_api::source::SourceId::new("source-1"),
            },
            "test",
        ))
        .await
        .expect("plan should succeed");
    let job_id = plan.job_id.clone();
    let request = PruneExecuteRequest {
        plan,
        confirm: true,
        reason: "test".to_string(),
    };
    let result = fake.execute(request).await.expect("execute should succeed");
    assert_eq!(result.job_id, job_id);
    assert_eq!(result.status, axon_api::source::LifecycleStatus::Completed);
}

#[tokio::test]
async fn fake_prune_service_dedupe_reports_completed() {
    let fake: Arc<dyn PruneService> = Arc::new(FakePruneService::new());
    let request = DedupeRequest {
        collection: None,
        response_mode: None,
    };
    let result = fake.dedupe(request).await.expect("dedupe should succeed");
    assert!(result.completed);
}

#[tokio::test]
async fn fake_prune_service_cleanup_debt_returns_result() {
    let fake: Arc<dyn PruneService> = Arc::new(FakePruneService::new());
    let result = fake
        .cleanup_debt(CleanupDebtRequest {
            source_id: Some("source-1".to_string()),
            dry_run: true,
        })
        .await
        .expect("cleanup_debt should succeed");
    assert_eq!(result.status, axon_api::source::LifecycleStatus::Completed);
}

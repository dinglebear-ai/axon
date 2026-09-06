use std::sync::Arc;

use super::*;

/// Compile-level assertion: `ResetServiceImpl` satisfies the trait and can
/// be built into a trait object.
#[allow(dead_code)]
fn assert_impl_is_reset_service(ctx: Arc<ServiceContext>) -> Arc<dyn ResetService> {
    Arc::new(ResetServiceImpl::new(ctx))
}

#[tokio::test]
async fn fake_reset_service_plan_then_execute() {
    let fake: Arc<dyn ResetService> = Arc::new(FakeResetService::new());
    let plan = fake.plan().await.expect("plan should succeed");
    assert_eq!(plan.plan_id, "fake-reset-plan-1");

    let result = fake
        .execute(&plan.plan_id, true, &ResetAuthz::admin())
        .await
        .expect("execute should succeed");
    assert!(!result.dry_run);
    assert_eq!(result.plan_id, plan.plan_id);
}

#[tokio::test]
async fn fake_reset_service_get_plan_returns_exact_dry_run_record() {
    let fake: Arc<dyn ResetService> = Arc::new(FakeResetService::new());
    let plan = fake.plan().await.expect("plan should succeed");
    let stored = fake
        .get_plan(&plan.plan_id)
        .await
        .expect("get should succeed");
    assert!(stored.dry_run);
    assert_eq!(stored.reset_plan, plan);
    assert_eq!(
        stored.inventory_checksum,
        stored.reset_plan.inventory_checksum
    );
}

#[tokio::test]
async fn fake_reset_service_requires_confirmation_and_admin() {
    let fake: Arc<dyn ResetService> = Arc::new(FakeResetService::new());
    let plan = fake.plan().await.expect("plan should succeed");
    assert!(
        fake.execute(&plan.plan_id, false, &ResetAuthz::admin())
            .await
            .is_err()
    );
    assert!(
        fake.execute(&plan.plan_id, true, &ResetAuthz::anonymous())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn fake_reset_service_plan_has_no_blockers() {
    let fake: Arc<dyn ResetService> = Arc::new(FakeResetService::new());
    let plan = fake.plan().await.expect("plan should succeed");
    assert!(plan.blockers.is_empty());
    assert!(!plan.stores.is_empty());
}

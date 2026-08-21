//! External-caller smoke for durable provider scheduling.

use axon_api::source::ProviderKind;
use axon_jobs::scheduler::{ProviderCapacityDomain, ProviderScheduler, SchedulerConfig};
use sqlx::SqlitePool;

#[tokio::test]
async fn provider_scheduler_public_contract_is_constructible() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").expect("lazy sqlite pool");
    let scheduler = ProviderScheduler::new(
        pool,
        ProviderCapacityDomain {
            kind: ProviderKind::Embedding,
            instance_id: "integration-tei".into(),
            authority_id: "integration-authority".into(),
        },
        SchedulerConfig {
            capacity: 2,
            interactive_reserve: 1,
            max_entries: 16,
            max_units: 16,
        },
    );
    assert!(scheduler.is_ok());
}

use super::*;

struct StubRunner;

#[async_trait]
impl UnifiedJobRunner for StubRunner {
    async fn run(
        &self,
        _claimed: &UnifiedClaimedJob,
        _store: &SqliteUnifiedJobStore,
        _shutdown: &CancellationToken,
    ) -> Result<(), ApiError> {
        Ok(())
    }
}

#[test]
fn empty_registry_returns_none_for_every_kind() {
    let registry = JobRunnerRegistry::new();
    assert!(registry.get(JobKind::Memory).is_none());
    assert!(!registry.contains(JobKind::ProviderProbe));
}

#[test]
fn registered_runner_is_retrievable_by_kind() {
    let mut registry = JobRunnerRegistry::new();
    registry.register(JobKind::Memory, Arc::new(StubRunner));

    assert!(registry.contains(JobKind::Memory));
    assert!(registry.get(JobKind::Memory).is_some());
    assert!(registry.get(JobKind::ProviderProbe).is_none());
}

#[test]
fn registering_same_kind_twice_replaces_the_runner() {
    let mut registry = JobRunnerRegistry::new();
    registry.register(JobKind::ProviderProbe, Arc::new(StubRunner));
    registry.register(JobKind::ProviderProbe, Arc::new(StubRunner));

    assert!(registry.contains(JobKind::ProviderProbe));
}

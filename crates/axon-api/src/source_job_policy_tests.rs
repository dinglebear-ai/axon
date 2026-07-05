use super::*;

#[test]
fn source_watch_extract_research_memory_graph_prune_provider_reset_are_job_backed() {
    for operation in [
        OperationKind::Source,
        OperationKind::Watch,
        OperationKind::Extract,
        OperationKind::Research,
        OperationKind::MemoryCompaction,
        OperationKind::MemoryImport,
        OperationKind::GraphMutation,
        OperationKind::Prune,
        OperationKind::ProviderProbe,
        OperationKind::Reset,
    ] {
        let policy = job_policy_for_operation(operation, JobExecutionMode::Detached);
        assert_eq!(policy, JobPolicy::JobBacked);
    }
}

#[test]
fn normal_query_and_retrieve_remain_jobless_until_long_running_work_is_requested() {
    assert_eq!(
        job_policy_for_operation(OperationKind::Query, JobExecutionMode::Foreground),
        JobPolicy::Synchronous
    );
    assert_eq!(
        job_policy_for_operation(OperationKind::Retrieve, JobExecutionMode::Foreground),
        JobPolicy::Synchronous
    );
    assert_eq!(
        job_policy_for_operation(OperationKind::Query, JobExecutionMode::LongRunningProvider),
        JobPolicy::JobBacked
    );
    assert_eq!(
        job_policy_for_operation(OperationKind::Retrieve, JobExecutionMode::ArtifactBacked),
        JobPolicy::JobBacked
    );
}

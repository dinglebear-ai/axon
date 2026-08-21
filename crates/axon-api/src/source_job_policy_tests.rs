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
fn query_and_retrieve_are_job_backed_in_every_execution_mode() {
    for operation in [OperationKind::Query, OperationKind::Retrieve] {
        for mode in [
            JobExecutionMode::Foreground,
            JobExecutionMode::Detached,
            JobExecutionMode::LongRunningProvider,
            JobExecutionMode::ArtifactBacked,
        ] {
            assert_eq!(
                job_policy_for_operation(operation, mode),
                JobPolicy::JobBacked,
                "{operation:?} should be job-backed in {mode:?}"
            );
        }
    }
}

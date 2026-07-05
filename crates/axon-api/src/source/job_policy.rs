use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Source,
    Watch,
    Extract,
    Research,
    MemoryCompaction,
    MemoryImport,
    GraphMutation,
    Prune,
    ProviderProbe,
    Reset,
    Query,
    Retrieve,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum JobExecutionMode {
    Foreground,
    Detached,
    LongRunningProvider,
    ArtifactBacked,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum JobPolicy {
    JobBacked,
    Synchronous,
}

pub fn job_policy_for_operation(operation: OperationKind, mode: JobExecutionMode) -> JobPolicy {
    match operation {
        OperationKind::Source
        | OperationKind::Watch
        | OperationKind::Extract
        | OperationKind::Research
        | OperationKind::MemoryCompaction
        | OperationKind::MemoryImport
        | OperationKind::GraphMutation
        | OperationKind::Prune
        | OperationKind::ProviderProbe
        | OperationKind::Reset => JobPolicy::JobBacked,
        OperationKind::Query | OperationKind::Retrieve => match mode {
            JobExecutionMode::Foreground => JobPolicy::Synchronous,
            JobExecutionMode::Detached
            | JobExecutionMode::LongRunningProvider
            | JobExecutionMode::ArtifactBacked => JobPolicy::JobBacked,
        },
    }
}

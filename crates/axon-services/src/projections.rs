//! Shared preflight and execution for the restored focused source projections.

use axon_api::source::*;
use axon_core::config::ProjectionBatchConfig;
use std::path::PathBuf;

#[path = "projections/events.rs"]
pub mod events;
#[path = "projections/execute.rs"]
pub mod execute;
#[path = "projections/limits.rs"]
pub mod limits;
#[path = "projections/preflight.rs"]
pub mod preflight;

pub use execute::{
    enqueue_source_projection_batch, execute_code_search_projection_batch,
    execute_source_projection_batch,
};
pub use preflight::{preflight_code_search_batch, preflight_source_batch};

pub type ProjectionBatchPolicy = ProjectionBatchConfig;

#[derive(Debug, Clone)]
pub struct SourceAccessPolicy {
    pub affinity: ExecutionAffinity,
    pub operator_allows_tool_execution: bool,
    pub allowed_roots: Option<Vec<PathBuf>>,
}

impl Default for SourceAccessPolicy {
    fn default() -> Self {
        Self {
            affinity: ExecutionAffinity::Inline,
            operator_allows_tool_execution: false,
            allowed_roots: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProjectionPreflight<T> {
    pub batch_id: BatchId,
    pub items: Vec<T>,
    pub aggregate_input_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct PreparedSourceItem {
    pub index: usize,
    pub request: SourceRequest,
    pub kind: SourceKind,
    pub route: RoutePlan,
    pub required_scope: AuthScope,
}

#[derive(Debug, Clone)]
pub struct PreparedCodeSearchItem {
    pub index: usize,
    pub plan: CodeSearchPlan,
}

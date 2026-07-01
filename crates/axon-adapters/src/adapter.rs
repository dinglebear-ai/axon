//! Source adapter boundary.

use async_trait::async_trait;
use axon_api::source::*;

use crate::capability::AdapterCapability;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait SourceAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    async fn capabilities(&self) -> Result<AdapterCapability>;
    async fn discover(&self, plan: &SourcePlan) -> Result<SourceManifest>;
    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &SourceManifestDiff,
    ) -> Result<SourceAcquisition>;
    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquisition: SourceAcquisition,
    ) -> Result<StageExecutionResult<Vec<SourceDocument>>>;
}

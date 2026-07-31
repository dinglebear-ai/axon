//! Source adapter boundary.

use async_trait::async_trait;
use axon_api::source::*;

use crate::acquisition::MaterializedSource;

pub type Result<T> = std::result::Result<T, ApiError>;

/// Monotonic progress snapshot for one adapter acquisition call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcquisitionProgress {
    /// Number of source items in this acquisition batch.
    pub items_total: u64,
    /// Number of source items whose acquisition attempt has finished.
    pub items_done: u64,
    /// Number of finished items that produced a document.
    pub documents_done: u64,
}

/// Optional observer used by adapters that can report progress before acquire returns.
#[async_trait]
pub trait AcquisitionProgressSink: Send + Sync {
    /// Persist or forward one monotonic acquisition snapshot.
    async fn report(&self, progress: AcquisitionProgress);
}

/// Version of the stable source-adapter contract described by the family matrix.
/// This is independent from the Axon crate release version.
pub const SOURCE_ADAPTER_CONTRACT_VERSION: &str = "1";

/// How an adapter wants a family-level 304/conditional-request reuse to be
/// handled by the shared runner. `None` (the default — what every non-web
/// family and `local` already do) means the runner never overlays prior
/// caching hints and always treats a manifest diff at face value. Only the
/// web adapter currently declares anything else (`web_source/reuse.rs`'s
/// `InProcessWebDocumentCache` + prior-etag overlay), because HTTP is the
/// only acquisition transport with a standard conditional-request contract
/// (`ETag`/`If-None-Match`) worth honoring — see finding C1's "custom fetch
/// retry/backoff" divergence axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReusePolicy {
    #[default]
    None,
    /// Overlay conditional-request metadata (e.g. a prior ETag) from the
    /// previous generation's manifest onto modified items before acquire, so
    /// the adapter's own fetch layer can skip re-downloading unchanged
    /// bodies.
    ConditionalRequest,
}

#[derive(Debug, Clone)]
pub struct GeneratedArchive {
    pub kind: ArtifactKind,
    pub content_type: String,
    pub bytes: Vec<u8>,
    pub content_hash: String,
    pub metadata: MetadataMap,
}

#[async_trait]
pub trait SourceAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    async fn capabilities(&self) -> Result<SourceAdapterCapability>;
    async fn discover(&self, plan: &SourcePlan) -> Result<SourceManifest>;
    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &SourceManifestDiff,
    ) -> Result<SourceAcquisition>;
    /// Acquire changed items while optionally emitting live batch progress.
    async fn acquire_with_progress(
        &self,
        plan: &SourcePlan,
        diff: &SourceManifestDiff,
        _progress: Option<&dyn AcquisitionProgressSink>,
    ) -> Result<SourceAcquisition> {
        self.acquire(plan, diff).await
    }
    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquisition: SourceAcquisition,
    ) -> Result<StageExecutionResult<Vec<SourceDocument>>>;

    /// Release adapter-owned state retained for this job after the pipeline
    /// reaches a terminal outcome. The shared runner calls this on success
    /// and failure; stateless adapters use the default no-op.
    fn release(&self, _plan: &SourcePlan) {}

    /// Adapter-owned materialization, run once before `discover`/`acquire`/
    /// `normalize`. Most families need this to validate/prepare acquisition
    /// state ahead of the shared pipeline (e.g. a shallow git clone, a
    /// bounded feed fetch, a validated session-export path) — those adapters
    /// override this method. Adapters with nothing to prepare ahead of time
    /// (e.g. `local`, whose identity/scope resolution happens before the
    /// plan is even built — see `axon-services/src/source/dispatch/local.rs`)
    /// use this default no-op passthrough.
    async fn materialize(&self, plan: SourcePlan) -> Result<MaterializedSource> {
        Ok(MaterializedSource::virtual_source(plan))
    }

    /// See [`ReusePolicy`]. Defaults to `None` — the shared runner never
    /// overlays conditional-request metadata unless an adapter opts in.
    fn reuse_policy(&self) -> ReusePolicy {
        ReusePolicy::None
    }

    fn wants_archive(&self, _plan: &SourcePlan) -> bool {
        false
    }

    /// Build an adapter-native durable archive from acquired items. The
    /// shared runner owns persistence and lifecycle metadata; adapters own
    /// only source-specific serialization. Stateless/non-archival adapters
    /// use the default no-op.
    fn build_archive(
        &self,
        _plan: &SourcePlan,
        _items: &[AcquiredSourceItem],
    ) -> Option<GeneratedArchive> {
        None
    }
}

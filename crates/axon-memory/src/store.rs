//! Memory store boundary and in-memory fake.
//!
//! The [`MemoryStore`] trait lives here; the in-memory [`FakeMemoryStore`]
//! implementation lives in the sibling [`fake`] module (kept out of this
//! file to stay under the repo's per-file monolith line cap).

use async_trait::async_trait;
use axon_api::source::*;

pub mod fake;
pub use fake::FakeMemoryStore;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn remember(&self, request: MemoryRequest) -> Result<MemoryResult>;
    async fn get(&self, memory_id: MemoryId) -> Result<Option<MemoryRecord>>;
    async fn load_many(&self, memory_ids: Vec<MemoryId>) -> Result<Vec<Option<MemoryRecord>>> {
        let mut records = Vec::with_capacity(memory_ids.len());
        for memory_id in memory_ids {
            records.push(self.get(memory_id).await?);
        }
        Ok(records)
    }
    async fn search(&self, request: MemorySearchRequest) -> Result<MemorySearchResult>;
    async fn context(&self, request: MemoryContextRequest) -> Result<MemoryContextResult>;
    async fn link(&self, request: MemoryLinkRequest) -> Result<MemoryResult>;
    async fn reinforce(
        &self,
        memory_id: MemoryId,
        signal: MemoryReinforcement,
    ) -> Result<MemoryResult>;

    /// Replace `memory_id` with `replacement_id`: mark the old memory
    /// `superseded`, point it at the replacement, and record history.
    async fn supersede(&self, request: MemorySupersedeRequest) -> Result<MemoryResult> {
        let _ = request;
        Err(unsupported_option("supersede"))
    }

    /// Flag two memories as conflicting; both transition to `contradicted` and
    /// enter the review queue.
    async fn contradict(&self, request: MemoryContradictRequest) -> Result<MemoryResult> {
        let _ = request;
        Err(unsupported_option("contradict"))
    }

    /// Transition a memory to a new status (archive/forget/pin/review/etc.).
    async fn set_status(&self, request: MemoryStatusRequest) -> Result<MemoryResult> {
        let _ = request;
        Err(unsupported_option("set_status"))
    }

    /// Return the current review queue.
    async fn review(&self, request: MemoryReviewRequest) -> Result<MemoryReviewResult> {
        let _ = request;
        Err(unsupported_option("review"))
    }

    /// Edit a memory's editable fields (body/title/type/confidence/salience/
    /// scope) in place.
    async fn update(&self, request: MemoryUpdateRequest) -> Result<MemoryResult> {
        let _ = request;
        Err(unsupported_option("update"))
    }

    /// Pin or unpin a memory (exempts it from decay while pinned).
    async fn pin(&self, request: MemoryPinRequest) -> Result<MemoryResult> {
        let _ = request;
        Err(unsupported_option("pin"))
    }

    /// Archive a memory (excluded from recall unless explicitly requested).
    async fn archive(&self, request: MemoryArchiveRequest) -> Result<MemoryResult> {
        let _ = request;
        Err(unsupported_option("archive"))
    }

    /// Forget a memory (never recalled again; history is preserved).
    async fn forget(&self, request: MemoryForgetRequest) -> Result<MemoryResult> {
        let _ = request;
        Err(unsupported_option("forget"))
    }

    /// Merge several memories into one new memory, recording provenance.
    async fn compact(&self, request: MemoryCompactRequest) -> Result<MemoryResult> {
        let _ = request;
        Err(unsupported_option("compact"))
    }

    /// Bulk-import memory records (or preview a dry-run plan).
    async fn import(&self, request: MemoryImportRequest) -> Result<MemoryImportResult> {
        let _ = request;
        Err(unsupported_option("import"))
    }

    /// Export memory records matching a scope.
    async fn export(&self, request: MemoryExportRequest) -> Result<MemoryExportResult> {
        let _ = request;
        Err(unsupported_option("export"))
    }

    async fn reset(&self) -> Result<()>;
    async fn capabilities(&self) -> Result<MemoryStoreCapability>;
}

pub(crate) fn unsupported_option(option: &str) -> ApiError {
    ApiError::new(
        "memory.unsupported_option",
        axon_error::ErrorStage::Retrieving,
        format!("fake memory store does not implement option {option}"),
    )
}

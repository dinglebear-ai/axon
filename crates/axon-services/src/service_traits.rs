//! The async orchestration service-trait seam for `axon-services`.
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! (14 traits). This module is a scaffold: it defines every contracted trait,
//! a `Fake<Name>` in-memory implementation of every method (suitable for
//! transport/parity tests), and — only where a real free function already
//! exists with a compatible or thinly-adaptable signature — a production
//! `<Name>Impl` that delegates to it. Methods with no existing free function
//! ("FAKE_ONLY" in the approved plan) or with a request/result DTO the
//! contract references but that has no faithful minimal analog yet ("SKIP")
//! still exist on the trait (so it stays object-safe/instantiable) but their
//! production implementation returns an error until a follow-up workstream
//! wires the real orchestration.
//!
//! Migrating CLI/MCP/REST callers onto these traits and implementing missing
//! domain behavior (e.g. real graph query orchestration) are explicitly out
//! of scope here — see the module doc comments in each submodule for the
//! per-method wrap/FAKE_ONLY/SKIP rationale.

pub mod ask_service;
pub mod collection_service;
pub mod document_service;
pub mod extract_service;
pub mod graph_service;
pub mod job_service;
pub mod memory_service;
pub mod provider_service;
pub mod prune_service;
pub mod query_service;
pub mod reset_service;
pub mod retrieve_service;
pub mod source_service;
pub mod watch_service;

pub use ask_service::{AskService, AskServiceImpl, FakeAskService};
pub use collection_service::{CollectionService, CollectionServiceImpl, FakeCollectionService};
pub use document_service::{DocumentService, DocumentServiceImpl, FakeDocumentService};
pub use extract_service::{ExtractService, ExtractServiceImpl, FakeExtractService};
pub use graph_service::{FakeGraphService, GraphService, GraphServiceImpl};
pub use job_service::{FakeJobService, JobService, JobServiceImpl};
pub use memory_service::{FakeMemoryService, MemoryService, MemoryServiceImpl};
pub use provider_service::{FakeProviderService, ProviderService, ProviderServiceImpl};
pub use prune_service::{FakePruneService, PruneService, PruneServiceImpl};
pub use query_service::{FakeQueryService, QueryService, QueryServiceImpl};
pub use reset_service::{FakeResetService, ResetService, ResetServiceImpl};
pub use retrieve_service::{FakeRetrieveService, RetrieveService, RetrieveServiceImpl};
pub use source_service::{FakeSourceService, SourceService, SourceServiceImpl};
pub use watch_service::{FakeWatchService, WatchService, WatchServiceImpl};

/// Shared "not implemented" error helper for production-impl stub methods
/// (FAKE_ONLY / SKIP methods per the service-contract wiring plan). Keeps the
/// message shape consistent across all 14 trait files.
pub(crate) fn not_implemented(trait_method: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "{trait_method} has no production orchestration yet (FAKE_ONLY/SKIP per \
         docs/pipeline-unification/foundation/types/service-contract.md); use the \
         Fake implementation for transport/parity tests until a follow-up workstream \
         wires the real free function"
    )
}

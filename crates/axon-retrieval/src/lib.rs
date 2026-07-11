//! Target pipeline retrieval boundary for PR9 vector/embedding scaffolding.
//!
//! Runtime RAG cutover stays in later issue #298 phases. The `RetrievalEngine`
//! boundary trait (`crate::boundary`) and its fake (`crate::testing`) are now
//! public — see `docs/pipeline-unification/foundation/types/trait-contract.md`
//! §RetrievalEngine — but nothing outside this crate consumes the trait yet;
//! existing runtime callers still go through `crate::engine::RetrievalEngine`'s
//! inherent API and `crate::service::run_query`.

#![allow(dead_code)]

pub mod ask_context;
pub mod boundary;
pub mod citation;
pub mod context;
pub mod engine;
pub mod filter;
pub mod graph;
pub mod memory;
pub mod plan;
pub mod publish;
pub mod query;
pub mod rank;
pub mod retrieve;
pub mod service;
pub mod testing;

pub use publish::{GenerationPublisher, InMemoryGenerationPublisher};
pub use retrieve::{RetrievedDocument, retrieve_document};
pub use service::{QueryServiceHit, QueryServiceRequest, QueryServiceResult, run_query};
pub use testing::{
    FakeGenerationPublisher, FakeGenerationPublisherMode, FakeRetrievalEngine, FakeRetrievalMode,
};

pub const CRATE_NAME: &str = "axon-retrieval";

#[cfg(test)]
#[path = "engine_tests.rs"]
mod engine_tests;
#[cfg(test)]
#[path = "generation_tests.rs"]
mod generation_tests;

#[cfg(test)]
#[path = "memory_tests.rs"]
mod memory_tests;

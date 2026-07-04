//! SourceGraph store for the unified Axon pipeline.
//!
//! `axon-graph` owns SourceGraph storage: nodes, edges, evidence, authority,
//! merge policy, conflict handling, and graph query helpers. It consumes
//! [`axon_api::source::GraphCandidate`] values produced by parsers/adapters and
//! never parses source files directly.
//!
//! Contracts:
//! - `docs/pipeline-unification/crates/axon-graph/CLAUDE.md`
//! - `docs/pipeline-unification/schemas/graph-schema.md`
//! - `docs/pipeline-unification/sources/source-graph.md`
//!
//! The closed node/edge/evidence kind registries live in [`node`], [`edge`],
//! and [`evidence`]. [`SqliteGraphStore`] is the durable implementation of the
//! [`store::GraphStore`] trait; [`store::FakeGraphStore`] is an in-memory fake
//! for higher-layer tests.

pub mod authority;
pub mod candidate;
pub mod edge;
pub mod error;
pub mod evidence;
pub mod merge;
pub mod migration;
pub mod node;
pub mod sqlite;
pub mod store;
pub mod testing;

pub use edge::GraphEdgeKind;
pub use evidence::EvidenceKind;
pub use node::GraphNodeKind;
pub use sqlite::SqliteGraphStore;
pub use store::{FakeGraphStore, GraphStore};

pub const CRATE_NAME: &str = "axon-graph";

pub mod schema_registry;
#[cfg(test)]
#[path = "store_tests.rs"]
mod store_tests;

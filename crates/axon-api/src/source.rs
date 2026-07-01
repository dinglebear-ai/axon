//! Transport-neutral source pipeline DTOs.
//!
//! These are data contracts only. They are shared by CLI, MCP, REST, jobs,
//! stores, providers, and future adapters without pulling runtime crates into
//! `axon-api`.

pub mod capability;
pub mod common;
pub mod document;
pub mod enums;
pub mod graph;
pub mod ids;
pub mod lifecycle;
pub mod listing;
pub mod stage;
pub mod state;
pub mod status;
pub mod vector;

pub use capability::*;
pub use common::*;
pub use document::*;
pub use enums::*;
pub use graph::*;
pub use ids::*;
pub use lifecycle::*;
pub use listing::*;
pub use stage::*;
pub use state::*;
pub use status::*;
pub use vector::*;

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "source_capability_tests.rs"]
mod capability_tests;

#[cfg(test)]
#[path = "source_status_tests.rs"]
mod status_tests;

#[cfg(test)]
#[path = "source_vector_tests.rs"]
mod vector_tests;

#[cfg(test)]
#[path = "source_graph_tests.rs"]
mod graph_tests;

#[cfg(test)]
#[path = "source_stage_fixture_tests.rs"]
mod stage_fixture_tests;

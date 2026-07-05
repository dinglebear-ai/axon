//! Transport-neutral source pipeline DTOs.
//!
//! These are data contracts only. They are shared by CLI, MCP, REST, jobs,
//! stores, providers, and future adapters without pulling runtime crates into
//! `axon-api`.

pub mod auth;
pub mod boundary;
pub mod capability;
pub mod common;
pub mod document;
pub mod enums;
pub mod graph;
pub mod ids;
pub mod job;
pub mod job_listing;
pub mod job_policy;
pub mod lifecycle;
pub mod listing;
pub mod llm;
pub mod memory;
pub mod provider_io;
pub mod prune;
pub mod stage;
pub mod state;
pub mod status;
pub mod vector;

pub use auth::*;
pub use boundary::*;
pub use capability::*;
pub use common::*;
pub use document::*;
pub use enums::*;
pub use graph::*;
pub use ids::*;
pub use job::*;
pub use job_listing::*;
pub use job_policy::*;
pub use lifecycle::*;
pub use listing::*;
pub use llm::*;
pub use memory::*;
pub use provider_io::*;
pub use prune::*;
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
#[path = "source_job_tests.rs"]
mod job_tests;

#[cfg(test)]
#[path = "source_job_dto_tests.rs"]
mod job_dto_tests;

#[cfg(test)]
#[path = "source_job_policy_tests.rs"]
mod job_policy_tests;

#[cfg(test)]
#[path = "source_vector_tests.rs"]
mod vector_tests;

#[cfg(test)]
#[path = "source_graph_tests.rs"]
mod graph_tests;

#[cfg(test)]
#[path = "source_stage_fixture_tests.rs"]
mod stage_fixture_tests;

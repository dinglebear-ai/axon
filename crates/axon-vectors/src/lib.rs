//! Vector store, payload, and point-batch boundary for the target source pipeline.
//!
//! PR9 gives this crate real contract-tested DTO conversion, payload validation,
//! deterministic fake-store behavior, and test-only Qdrant conversion helpers.
//! Existing public runtime paths remain in the legacy crates until a later
//! cutover PR.

pub mod collection;
pub mod filter;
pub mod health;
pub mod payload;
mod payload_redaction;
pub mod point;
#[cfg(test)]
mod qdrant;
pub mod query;
mod sparse;
pub mod store;
mod store_helpers;
pub mod testing;
mod validation;

pub const CRATE_NAME: &str = "axon-vectors";

#[cfg(test)]
#[path = "store_tests.rs"]
mod store_tests;

#[cfg(test)]
#[path = "store_mode_tests.rs"]
mod store_mode_tests;

#[cfg(test)]
#[path = "payload_tests.rs"]
mod payload_tests;

#[cfg(test)]
#[path = "point_tests.rs"]
mod point_tests;

#[cfg(test)]
#[path = "local_payload_tests.rs"]
mod local_payload_tests;

#[cfg(test)]
#[path = "qdrant_tests.rs"]
mod qdrant_tests;

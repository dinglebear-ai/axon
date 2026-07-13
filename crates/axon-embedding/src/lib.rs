//! Embedding provider boundary for the target source pipeline.
//!
//! PR9 gives this crate real contract-tested provider traits, deterministic
//! fakes, provider capabilities, reservation metadata, and non-wired provider
//! shells. Existing public runtime paths remain in the legacy crates until a
//! later cutover PR.

#![allow(clippy::result_large_err)]

pub mod batch;
pub mod capability;
pub mod fake;
pub mod openai_compat;
pub mod provider;
pub mod reservation;
pub mod tei;
pub mod testing;

pub const CRATE_NAME: &str = "axon-embedding";

#[cfg(test)]
#[path = "provider_tests.rs"]
mod provider_tests;

#[cfg(test)]
#[path = "tei_client_tests.rs"]
mod tei_client_tests;

#[cfg(test)]
#[path = "reservation_compat_tests.rs"]
mod reservation_compat_tests;

#[cfg(test)]
#[path = "capability_tests.rs"]
mod capability_tests;

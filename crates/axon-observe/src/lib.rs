//! Observation, progress, heartbeat, and provider reservation primitives for the
//! unified source pipeline.

pub mod collector;
pub mod event;
pub mod heartbeat;
pub mod log;
pub mod metric;
pub mod phase;
pub mod progress;
pub mod reservation;
pub mod span;
pub mod testing;

pub const CRATE_NAME: &str = "axon-observe";

#[cfg(test)]
#[path = "reservation_tests.rs"]
mod reservation_tests;

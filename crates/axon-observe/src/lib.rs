//! Observation, progress, heartbeat, and provider reservation primitives for the
//! unified source pipeline.

pub mod collector;
pub mod event;
pub mod heartbeat;
pub mod log;
pub mod metric;
pub mod migration;
pub mod phase;
pub mod progress;
pub mod redaction;
pub mod reservation;
pub mod schema_registry;
pub mod security_audit;
pub mod sequence;
pub mod sink;
pub mod source_metrics;
pub mod span;
pub mod testing;

pub const CRATE_NAME: &str = "axon-observe";

#[cfg(test)]
#[path = "reservation_tests.rs"]
mod reservation_tests;

#[cfg(test)]
#[path = "collector_tests.rs"]
mod collector_tests;

#[cfg(test)]
#[path = "event_tests.rs"]
mod event_tests;

#[cfg(test)]
#[path = "heartbeat_tests.rs"]
mod heartbeat_tests;

#[cfg(test)]
#[path = "security_audit_tests.rs"]
mod security_audit_tests;

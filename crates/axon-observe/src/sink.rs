//! Production [`ObservabilitySink`](crate::collector::ObservabilitySink)
//! implementations.
//!
//! Two real, non-test sinks live here:
//!
//! - [`SqliteObservabilitySink`] — persists durable event rows, upserts the
//!   active heartbeat row, and records provider degradation to SQLite. It owns
//!   an in-crate migration so it is usable standalone (Qdrant/TEI not required).
//! - [`TracingObservabilitySink`] — forwards the same event model to the
//!   `tracing` subscriber as structured, redaction-safe log fields.
//!
//! Both stamp a monotonic per-`job_id` sequence via a shared
//! [`SequenceRegistry`](crate::sequence::SequenceRegistry) at emit time, fixing
//! the previous hardcoded `sequence: 1` placeholder produced by the pure
//! builders in [`crate::event`].

pub mod sqlite;
pub mod tracing_sink;

pub use sqlite::{ProviderHealthRecord, SqliteObservabilitySink};
pub use tracing_sink::TracingObservabilitySink;

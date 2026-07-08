//! Vector-payload adapter over the shared `axon-core` redaction boundary.
//!
//! The `Redactor` trait, `RedactionReport`, and `DefaultRedactor` used to
//! live here as a vector-payload-only implementation. They now live in
//! `axon_core::redact` so every crate above `axon-core` (jobs, memory,
//! graph, cli, mcp, web) shares one boundary instead of re-implementing
//! detection per crate. This module just re-exports them and keeps the
//! vector-payload-specific `RedactionContext::vector_payload()` entry point
//! discoverable from `crate::redactor` for existing call sites.

pub use axon_core::redact::{
    DefaultRedactor, REDACTION_PLACEHOLDER, REDACTION_VERSION, RedactionContext, RedactionReport,
    RedactionStatus, RedactionSurface, Redactor, redact_metadata,
};

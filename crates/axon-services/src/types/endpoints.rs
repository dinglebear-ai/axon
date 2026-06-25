//! Web-discovery endpoint types — re-exported from `core`.
//!
//! These types are owned by `axon_core::content::endpoints` (the extractor
//! that produces them). They are re-exported here so existing
//! `crate::types::{EndpointKind, DiscoveredEndpoint, ...}` callers
//! across the services, web, mcp, and cli layers keep compiling unchanged.
//! Keeping `core` the single owner preserves the `core` → `services` dependency
//! direction (core must never depend on services).

pub use axon_core::content::{
    DiscoveredEndpoint, EndpointKind, EndpointOptions, EndpointReport, EndpointSourceKind,
    EndpointVerification, McpCandidateAttempt, McpHostKind, McpProbeOutcome, RpcProbeResult,
    RpcProtocol, RpcTransport,
};

#[cfg(test)]
#[path = "endpoints_tests.rs"]
mod tests;

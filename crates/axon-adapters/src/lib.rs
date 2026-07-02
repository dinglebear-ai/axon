//! Target pipeline crate skeleton for `axon-adapters`.
//!
//! This crate is intentionally marker-only in PR0. Runtime behavior moves here
//! in issue #298 implementation PRs after contract tests exist.

pub mod acquisition;
pub mod adapter;
pub mod boundary;
pub mod capability;
pub mod cli_tool;
pub mod feed;
pub mod git;
pub mod local;
mod local_select;
pub mod manifest;
pub mod mcp_tool;
pub mod reddit;
pub mod registry;
pub mod registry_sources;
pub mod sessions;
pub mod testing;
pub mod web;
pub mod youtube;

pub use acquisition::{AcquiredItem, AcquisitionManifest, FetchStatus};
pub use adapter::SourceAdapter;
pub use capability::{AdapterCapability, AdapterVersion};
pub use registry::SourceAdapterRegistry;
pub use testing::FakeSourceAdapter;

pub const CRATE_NAME: &str = "axon-adapters";

#[cfg(test)]
#[path = "adapter_tests.rs"]
mod adapter_tests;

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod manifest_tests;

#[cfg(test)]
#[path = "local_tests.rs"]
mod local_tests;

#[cfg(test)]
#[path = "local_test_support.rs"]
mod local_test_support;

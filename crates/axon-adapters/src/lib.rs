//! Target pipeline crate skeleton for `axon-adapters`.
//!
//! This crate is intentionally marker-only in PR0. Runtime behavior moves here
//! in issue #298 implementation PRs after contract tests exist.

pub mod acquisition;
pub mod adapter;
pub mod boundary;
pub mod capability;
pub mod cli_tool;
pub mod enrichment;
pub mod family_matrix;
pub mod feed;
pub mod git;
pub mod local;
mod local_select;
pub mod manifest;
pub mod mcp_tool;
pub mod onboarding;
pub mod reddit;
pub mod registry;
pub mod registry_sources;
pub mod sessions;
pub mod spec;
pub mod testing;
pub mod web;
pub mod youtube;

pub use acquisition::{AcquiredItem, AcquisitionManifest, FetchStatus};
pub use adapter::SourceAdapter;
pub use capability::{AdapterCapability, AdapterVersion};
pub use enrichment::{NoopSourceEnricher, SourceEnricher};
pub use family_matrix::{SourceFamilyMatrix, source_family_matrix};
pub use onboarding::{OnboardingRow, SourceOnboardingStatus, onboarding_rows, onboarding_status};
pub use registry::SourceAdapterRegistry;
pub use spec::{ParserFamily, SourceAdapterSpec, SourceFamily, SourceScopeCapability};
pub use testing::{
    FakeSourceAdapter, FakeSourceAdapterMode, FakeSourceEnricher, FakeSourceEnricherMode,
};

pub const CRATE_NAME: &str = "axon-adapters";

#[cfg(test)]
#[path = "adapter_tests.rs"]
mod adapter_tests;

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod manifest_tests;

#[cfg(test)]
#[path = "family_matrix_tests.rs"]
mod family_matrix_tests;

#[cfg(test)]
#[path = "fixture_tests.rs"]
mod fixture_tests;

#[cfg(test)]
#[path = "onboarding_tests.rs"]
mod onboarding_tests;

#[cfg(test)]
#[path = "tool_tests.rs"]
mod tool_tests;

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod mcp_tests;

#[cfg(test)]
#[path = "local_tests.rs"]
mod local_tests;

#[cfg(test)]
#[path = "local_test_support.rs"]
mod local_test_support;

#[cfg(test)]
#[path = "web_tests.rs"]
mod web_tests;

#[cfg(test)]
#[path = "registry_sources_test_support.rs"]
mod registry_sources_test_support;

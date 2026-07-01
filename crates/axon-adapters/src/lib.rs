//! Target pipeline crate skeleton for `axon-adapters`.
//!
//! This crate is intentionally marker-only in PR0. Runtime behavior moves here
//! in issue #298 implementation PRs after contract tests exist.

pub mod acquisition;
pub mod adapter;
pub mod capability;
pub mod cli_tool;
pub mod feed;
pub mod git;
pub mod local;
pub mod manifest;
pub mod mcp_tool;
pub mod reddit;
pub mod registry;
pub mod registry_sources;
pub mod sessions;
pub mod testing;
pub mod web;
pub mod youtube;

pub const CRATE_NAME: &str = "axon-adapters";

//! Target pipeline crate skeleton for `axon-parse`.
//!
//! This crate is intentionally marker-only in PR0. Runtime behavior moves here
//! in issue #298 implementation PRs after contract tests exist.

pub mod code;
pub mod config;
pub mod docker;
pub mod env;
pub mod facts;
pub mod graph_candidate;
pub mod manifest;
pub mod parser;
pub mod registry;
pub mod schema;
pub mod session;
pub mod testing;
pub mod tool;

pub const CRATE_NAME: &str = "axon-parse";

//! Target pipeline crate skeleton for `axon-error`.
//!
//! This crate is intentionally marker-only in PR0. Runtime behavior moves here
//! in issue #298 implementation PRs after contract tests exist.

pub mod api_error;
pub mod code;
pub mod context;
pub mod conversion;
pub mod cooling;
pub mod degradation;
pub mod retry;
pub mod severity;
pub mod stage;
pub mod testing;

pub const CRATE_NAME: &str = "axon-error";

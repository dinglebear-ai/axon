//! Target pipeline crate skeleton for `axon-llm`.
//!
//! This crate is intentionally marker-only in PR0. Runtime behavior moves here
//! in issue #298 implementation PRs after contract tests exist.

pub mod capability;
pub mod codex;
pub mod completion;
pub mod fake;
pub mod gemini;
pub mod openai_compat;
pub mod prompt;
pub mod provider;
pub mod stream;
pub mod testing;

pub const CRATE_NAME: &str = "axon-llm";

#[cfg(test)]
#[path = "provider_tests.rs"]
mod provider_tests;

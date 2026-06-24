//! The `axon` binary crate.
//!
//! All CLI dispatch and command handlers now live in the `axon-cli` crate; this
//! crate is the thin binary wrapper (entry point in `main.rs`) plus the
//! build-time web-asset embedding (`build.rs`). `run()` is re-exported so
//! `main.rs` (and integration tests) keep calling `axon::run()` unchanged.

pub use axon_cli::run;

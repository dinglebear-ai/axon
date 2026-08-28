//! Typed Codex app-server protocol and trusted-control domain.
//!
//! This crate is deliberately independent from Axon's LLM completion types.
//! The existing synthesis backend may share protocol primitives, but control
//! runtimes must never share its pool, home, queues, or lifecycle.

pub mod capabilities;
pub mod control;
pub mod protocol;

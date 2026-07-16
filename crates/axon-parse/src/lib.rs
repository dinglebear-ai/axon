//! Target pipeline crate for `axon-parse` (issue #298).
//!
//! Live, not marker-only: per-family parsers (`code`, `config`, `docker`,
//! `env`, `markdown`, `session`, `tool`), fact extraction (`facts`), and
//! `graph_candidate` construction are wired and tested in this crate.

#![allow(clippy::too_many_arguments)]

pub mod builtins;
pub mod code;
pub mod config;
pub mod docker;
pub mod env;
pub mod facts;
pub mod graph_candidate;
pub mod manifest;
pub mod markdown;
pub mod parser;
pub mod registry;
pub mod schema;
pub mod session;
pub mod testing;
pub mod tool;
pub mod tool_schema;
pub mod validate;
pub mod vertical;

pub const CRATE_NAME: &str = "axon-parse";

#[cfg(test)]
#[path = "parser_tests.rs"]
mod parser_tests;

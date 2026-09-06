//! Vertical-extractor framework (axon_rust-upnq).
//!
//! Implements specialized per-site extraction functions consumed by source
//! adapters. URL/name matching order and dispatch policy belong to
//! `axon-adapters`; this crate owns only extractor implementations and their
//! narrow shared context/output types.
//!
//! ## Design (plain-module dispatch, no trait objects)
//! Each vertical is a plain module exposing `INFO`, `matches()`, and
//! `extract()`. `axon-adapters::vertical_registry` composes those functions
//! into acquisition routing without giving this implementation crate
//! pipeline ownership.
//!
//! ## Module layout
//! ```text
//! src/lib.rs             — public API
//! src/context.rs         — narrow extractor context
//! src/error.rs           — vertical error taxonomy
//! src/types.rs           — output and descriptor types
//! src/verticals.rs       — vertical module declarations
//! src/verticals/*.rs     — provider-specific implementations
//! ```

mod context;
mod error;
mod git_payload;
mod types;
pub mod verticals;

pub use context::{VerticalContext, VerticalCredentials};
pub use error::VerticalError;
pub use types::{ExtractorInfo, ScrapedDoc};

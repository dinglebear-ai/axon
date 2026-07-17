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
//! src/extract.rs         — this file (public API)
//! src/extract/
//!   context.rs           — VerticalContext (narrowed ServiceContext view)
//!   error.rs             — VerticalError (re-export from a9l6 taxonomy)
//!   types.rs             — ScrapedDoc, ExtractorInfo
//!   verticals.rs         — declares all vertical sub-modules
//!   verticals/
//!     github_repo.rs     — reference extractor (di8j)
//!     (more added by di8j / 25cu / jj43 / urk2 beads)
//! ```

mod context;
mod error;
mod git_payload;
mod types;
pub mod verticals;

pub use context::VerticalContext;
pub use error::VerticalError;
pub use types::{ExtractorInfo, ScrapedDoc};

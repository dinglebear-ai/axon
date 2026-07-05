//! Vertical-extractor framework (axon_rust-upnq).
//!
//! Routes a URL to a specialized per-site extractor module before the
//! generic HTTP scrape pipeline runs. When an extractor claims the URL
//! and succeeds, the caller uses the richer `ScrapedDoc` output. On
//! `None` or `Err`, fall through to the generic path.
//!
//! ## Design (plain-module dispatch, no trait objects)
//! Each vertical is a plain module exposing `INFO`, `matches()`, and
//! `extract()`. A match-chain in `registry.rs` is fast, readable, and
//! avoids dyn-dispatch overhead. See the exhaustiveness test in
//! `registry.rs` for the compile-time-ish guarantee that replaces
//! trait enforcement.
//!
//! ## Module layout
//! ```text
//! src/extract.rs         — this file (public API)
//! src/extract/
//!   context.rs           — VerticalContext (narrowed ServiceContext view)
//!   error.rs             — VerticalError (re-export from a9l6 taxonomy)
//!   registry.rs          — dispatch_by_url / dispatch_by_name / list
//!   types.rs             — ScrapedDoc, ExtractorInfo
//!   verticals.rs         — declares all vertical sub-modules
//!   verticals/
//!     github_repo.rs     — reference extractor (di8j)
//!     (more added by di8j / 25cu / jj43 / urk2 beads)
//! ```

mod context;
mod error;
mod registry;
pub mod scrape;
pub mod sync;
mod types;
mod verticals;

pub use context::VerticalContext;
pub use error::VerticalError;
pub use registry::{dispatch_by_name, dispatch_by_url, list as list_extractors};
pub use types::{ExtractorInfo, ScrapedDoc};

#[cfg(test)]
#[path = "vertical_parse_facts_tests.rs"]
mod vertical_parse_facts_tests;

//! Registered vertical extractors.
//!
//! Each sub-module is a plain module (no trait, no dyn dispatch) exposing:
//! - `pub const INFO: ExtractorInfo` — catalog entry
//! - `pub fn matches(url: &str) -> bool` — URL claim predicate
//! - `pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError>`

pub mod github_repo;

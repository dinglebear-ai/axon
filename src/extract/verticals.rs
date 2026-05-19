//! Registered vertical extractors.
//!
//! Each sub-module is a plain module (no trait, no dyn dispatch) exposing:
//! - `pub const INFO: ExtractorInfo` — catalog entry
//! - `pub fn matches(url: &str) -> bool` — URL claim predicate
//! - `pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError>`

pub mod amazon;
pub mod arxiv;
pub mod crates_io;
pub mod dev_to;
pub mod docker_hub;
pub mod docs_rs;
pub mod ebay;
pub mod github_issue;
pub mod github_pr;
pub mod github_release;
pub mod github_repo;
pub mod hackernews;
pub mod huggingface_model;
pub mod npm;
pub mod pypi;
pub mod reddit;
pub mod shopify;
pub mod stackoverflow;
pub mod youtube_video;

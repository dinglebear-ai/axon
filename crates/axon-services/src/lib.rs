pub mod action_api;
pub mod artifacts;
pub mod brand;
pub mod client_contract;
pub mod code_search_watch;
pub mod config;
pub mod context;
pub mod crawl;
pub mod crawl_sync;
pub mod debug;
pub mod diff;
pub mod document;
pub mod embed;
pub mod endpoints;
pub mod events;
pub mod extract;
pub mod feed_acquire;
pub use feed_acquire::fetch_feed_to_file;
#[allow(dead_code)]
pub(crate) mod feed_source;
pub use feed_source::{FeedSourceIndexInput, FeedSourceIndexOutput, index_feed_source_with_job};
pub mod feed_target;
pub use feed_target::{is_feed_target, normalize_feed_target};
pub mod freshness;
pub mod git_acquire;
pub use git_acquire::{clone_git_repo, is_git_target};
#[allow(dead_code)]
pub(crate) mod git_source;
pub use git_source::{GitSourceIndexInput, GitSourceIndexOutput, index_git_source_with_job};
pub mod ingest;
pub mod jobs;
pub(crate) mod local_source;
pub use local_source::{
    LocalSourceIndexInput, LocalSourceIndexOutput, LocalSourceSelectionPolicy,
    index_local_source_with_job,
};
pub mod map;
pub mod memory;
pub mod migrate;
pub mod mobile_sessions;
pub mod query;
pub mod reddit_acquire;
pub use reddit_acquire::fetch_reddit_dump;
#[allow(dead_code)]
pub(crate) mod reddit_source;
pub use reddit_source::{
    RedditSourceIndexInput, RedditSourceIndexOutput, index_reddit_source_with_job,
};
pub mod reddit_target;
pub use reddit_target::is_reddit_target;
pub mod refresh;
pub mod registry_acquire;
pub use registry_acquire::{fetch_registry_dump, is_registry_target, parse_registry_target};
#[allow(dead_code)]
pub(crate) mod registry_source;
pub use registry_source::{
    RegistrySourceIndexInput, RegistrySourceIndexOutput, index_registry_source_with_job,
};
pub mod runtime;
pub mod scrape;
pub mod screenshot;
pub mod search;
pub mod search_crawl;
pub mod sessions;
#[allow(dead_code)]
pub(crate) mod sessions_source;
pub use sessions_source::{
    SessionsSourceIndexInput, SessionsSourceIndexOutput, index_sessions_source_with_job,
};
pub mod sessions_target;
pub use sessions_target::{SessionSelector, is_session_selector, parse_session_selector};
pub mod setup;
pub mod source;
pub use source::index_source;
pub mod source_jobs;
pub mod summarize;
pub mod sync;
pub mod system;
pub mod transport;
pub mod types;
pub mod watch;
pub(crate) mod web_source;
pub use web_source::{
    WebSourceIndexInput, WebSourceIndexOutput, index_web_source, index_web_source_with_job,
};
pub mod youtube_acquire;
pub use youtube_acquire::fetch_youtube_dump;
#[allow(dead_code)]
pub(crate) mod youtube_source;
pub use youtube_source::{
    YoutubeSourceIndexInput, YoutubeSourceIndexOutput, index_youtube_source_with_job,
};
pub mod youtube_target;
pub use youtube_target::is_youtube_target;

#[cfg(test)]
#[path = "client_contract_tests.rs"]
mod client_contract_tests;
#[cfg(test)]
#[path = "freshness_tests.rs"]
mod freshness_tests;
#[cfg(test)]
#[path = "sync_tests.rs"]
mod sync_tests;
#[cfg(test)]
pub(crate) mod test_support;

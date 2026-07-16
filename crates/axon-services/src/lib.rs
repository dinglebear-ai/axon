// `service_traits::source_service::SourceServiceImpl` boxes `crate::source::
// index_source`'s already-deep async call chain a second time via
// `#[async_trait]`; the extra layer pushes query-depth layout computation
// past the default recursion limit (128). Raised, not worked around.
#![recursion_limit = "256"]
#![allow(unused_imports, unused_qualifications)]
#![allow(
    clippy::await_holding_lock,
    clippy::clone_on_copy,
    clippy::collapsible_if,
    clippy::doc_lazy_continuation,
    clippy::field_reassign_with_default,
    clippy::question_mark,
    clippy::result_large_err,
    clippy::too_many_arguments
)]

pub mod action_api;
pub mod artifacts;
pub mod brand;
pub mod client_contract;
pub mod code_search_watch;
pub mod config;
pub mod config_snapshot_hash;
pub mod context;
pub(crate) mod contract_write;
pub mod debug;
pub mod diff;
pub mod document;
pub mod endpoints;
pub mod events;
pub mod extract;
pub mod feed_target;
pub mod graph;
pub use feed_target::{is_feed_target, normalize_feed_target};
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
pub mod reddit_target;
pub use reddit_target::is_reddit_target;
pub mod prune;
pub mod reset;
pub use reset::reset;
pub mod runtime;
pub mod scrape;
pub mod screenshot;
pub mod search;
pub mod search_crawl;
pub mod search_source_index;
pub mod service_traits;
pub mod sessions;
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
pub mod uploads;
pub mod watch;
pub(crate) mod web_source;
pub use web_source::{
    WebSourceIndexInput, WebSourceIndexOutput, index_web_source, index_web_source_with_job,
};
pub mod youtube_target;

pub use youtube_target::is_youtube_target;

#[cfg(test)]
#[path = "client_contract_tests.rs"]
mod client_contract_tests;
#[cfg(test)]
#[path = "source_observability_tests.rs"]
mod source_observability_tests;
#[cfg(test)]
#[path = "source_web_job_identity_tests.rs"]
mod source_web_job_identity_tests;
#[cfg(test)]
#[path = "sync_tests.rs"]
mod sync_tests;
#[cfg(test)]
pub(crate) mod test_support;

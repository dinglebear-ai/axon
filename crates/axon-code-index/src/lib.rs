pub mod config;
pub mod ensure;
pub mod indexer;
pub mod manifest;
mod paths;
pub mod progress;
pub mod store;
mod store_schema;
mod summary;

pub use config::{CodeIndexIdentity, CodeSearchAllowedRoots};
pub use ensure::{FreshnessWarning, ensure_fresh};
pub use progress::{ReindexProgress, ReindexProgressSink};

#[cfg(test)]
#[path = "code_index_tests.rs"]
mod tests;

pub mod config;
pub mod ensure;
pub mod indexer;
pub mod manifest;
pub mod store;
mod store_schema;

pub use config::{CodeIndexIdentity, CodeSearchAllowedRoots};
pub use ensure::{FreshnessWarning, ensure_fresh};

#[cfg(test)]
#[path = "code_index_tests.rs"]
mod tests;

pub(crate) mod config;
pub(crate) mod ensure;
pub(crate) mod indexer;
pub(crate) mod manifest;
pub(crate) mod store;
mod store_schema;

pub(crate) use config::{CodeIndexIdentity, CodeSearchAllowedRoots};
pub(crate) use ensure::{FreshnessWarning, ensure_fresh};

#[cfg(test)]
#[path = "code_index_tests.rs"]
mod tests;

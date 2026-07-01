pub mod artifacts;
pub mod ask_explain;
pub mod binary_status;
pub mod boundary;
pub mod config;
pub mod content;
pub mod endpoints;
pub mod env;
pub mod error;
pub mod events;
pub mod health;
pub mod http;
pub mod llm;
pub mod logging;
pub mod paths;
pub mod redact;
pub mod sqlite;
pub mod structured;
pub mod ui;

/// Local code-search index schema version. Bumping it invalidates stored
/// `local_index_version` payloads so stale code chunks are re-indexed. Lives
/// here so both `code_index` and `vector` can reference it without a cycle.
pub const CODE_INDEX_VERSION: u32 = 1;

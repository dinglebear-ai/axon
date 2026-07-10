//! Pipeline/transport stage each error is attributed to.
//!
//! Stage values come from the "Stage Values" table in
//! `docs/pipeline-unification/runtime/error-handling.md`. Serialized JSON names
//! are stable snake_case and must not change without a schema revision.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Pipeline/transport stage an error is attributed to.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ErrorStage {
    /// CLI/MCP/REST request parse, removed command/action.
    Parsing,
    /// Missing fields, bad types, unsupported flags.
    Validation,
    /// Source resolution, canonical URI, authority.
    Resolving,
    /// Adapter/scope/provider selection.
    Routing,
    /// Auth, credentials, execution policy.
    Authorizing,
    /// Source plan, prune plan.
    Planning,
    /// Job/watch/source lease.
    Leasing,
    /// Manifest/map discovery.
    Discovering,
    /// Manifest diff.
    Diffing,
    /// HTTP/git/package/local/MCP/CLI fetch.
    Fetching,
    /// Browser/CDP/render provider.
    Rendering,
    /// Enrichment (metadata/classification/summary/extraction/authority).
    Enriching,
    /// SourceDocument creation.
    Normalizing,
    /// Parser facts/chunk parser (serialized as `parsing_content`).
    ParsingContent,
    /// Graph writes/merge/conflict.
    Graphing,
    /// Chunking/PreparedDocument.
    Preparing,
    /// Batch assembly for embedding/vectorizing.
    Batching,
    /// Embedding provider/batch.
    Embedding,
    /// Vector point construction/payload build.
    Vectorizing,
    /// VectorStore writes.
    Upserting,
    /// Generation publish.
    Publishing,
    /// Cleanup/prune/dedupe.
    Cleaning,
    /// Query/retrieve context.
    Retrieving,
    /// LLM synthesis.
    Synthesizing,
    /// RAG/evaluation scoring.
    Evaluating,
    /// Progress/log/status emit.
    Observing,
    /// Storage boundary (ledger/artifact/job store), phase-contextual.
    Storage,
    /// Provider boundary (embedding/llm/render/search), phase-contextual.
    Provider,
    /// Transport boundary (CLI/REST/MCP dispatch), phase-contextual.
    Transport,
    /// Internal/unclassified failure within the active operation phase.
    Internal,
}

#[cfg(test)]
#[path = "stage_tests.rs"]
mod tests;

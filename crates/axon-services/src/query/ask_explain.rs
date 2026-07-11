//! Bridge to the legacy `ask --explain` reranker pipeline (used by `train`).
//!
//! **Deliberately deferred, disclosed scope cut for the #298 read+synthesis
//! migration.** Everything else `ask`/`evaluate`/`suggest`/`retrieve` need is
//! ported off `axon_vector::` in this crate (see `synthesis.rs`,
//! `evaluate.rs`, `suggest.rs`, `retrieve.rs`) — this one call is the sole
//! remaining `axon_vector::` dependency in the `query` module tree, isolated
//! here rather than left inline in `ask_retrieval.rs`.
//!
//! `ask --explain` traces the legacy candidate reranker's per-candidate
//! scoring/filter/selection decisions (`AskExplainTrace`) for `train`'s
//! tuning workflow. That reranker (`axon_vector::ops::commands::ask::
//! build_ask_context` + its `context/{build,dedup,heuristics,query_rewrite,
//! retrieval}` submodules, ~4,300 non-test lines) depends on low-level
//! Qdrant dual-search dispatch and candidate-scoring primitives
//! (`axon_vector::ops::commands::retrieval`, `axon_vector::ops::ranking`)
//! that are **also** the implementation of `axon-vector`'s `code_search` and
//! legacy `query_hits` — both out of this migration's scope and required to
//! keep working unmodified. Porting the reranker here would require either:
//!
//! 1. Duplicating that shared Qdrant/TEI dispatch + ranking layer into
//!    `axon-retrieval` (a multi-thousand-line copy of the raw REST client
//!    code `axon-vectors`/`axon-embedding` already exist to replace, and a
//!    second copy to keep in sync until `code_search`/legacy `query_hits`
//!    are migrated in a later slice), or
//! 2. Re-deriving the explain trace from the new `axon-retrieval` engine's
//!    hybrid RRF results, which do not carry the same per-candidate
//!    rerank-score/filter-decision/selection-decision detail the legacy
//!    reranker computes — this would change `train`'s tuning signal, not
//!    just its plumbing.
//!
//! Both are out of scope for this slice. `ask --explain` therefore keeps
//! running the full legacy pipeline (retrieval + reranking + synthesis, none
//! of it cut over) exactly as it did before this port, and this module is
//! the one place that dependency is declared.

use axon_core::config::Config;
use std::error::Error;

use crate::types::AskResult;

/// Run the full legacy `ask` pipeline (legacy reranker + legacy synthesis),
/// used only for `cfg.ask_explain` requests.
pub(crate) async fn ask_result_via_legacy_explain(
    cfg: &Config,
    query: &str,
) -> Result<AskResult, Box<dyn Error>> {
    axon_vector::ops::commands::ask::ask_result(cfg, query)
        .await
        .map_err(|e| -> Box<dyn Error> { e.to_string().into() })
}

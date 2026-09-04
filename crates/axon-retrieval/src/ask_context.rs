//! Ask-context DTO for [`crate::boundary::RetrievalEngine::build_ask_context`].
//!
//! Required by `docs/pipeline-unification/foundation/types/crate-structure.md`'s
//! `axon-retrieval` module list; did not previously exist as a file.

use crate::citation::Citation;
use crate::context::ContextBundle;
use crate::query::RetrievalResult;

pub const MODULE_NAME: &str = "ask_context";

/// Composed retrieval context for an `ask` (RAG synthesis) request: the fused
/// context bundle handed to the LLM, the citations backing it, and the full
/// underlying [`RetrievalResult`] for callers that need the raw matches/plan.
#[derive(Debug, Clone, PartialEq)]
pub struct AskContext {
    pub context: ContextBundle,
    pub citations: Vec<Citation>,
    pub retrieval: RetrievalResult,
}

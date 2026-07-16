//! `action=prune` MCP wire request — split out of `requests.rs` to stay under
//! the monolith file-size cap.

use serde::{Deserialize, Serialize};

use super::ResponseMode;

/// `action=prune` — canonical cleanup planning and execution
/// (`docs/pipeline-unification/surfaces/tool-contract.md` "Prune, Collections,
/// Graph, and Providers Actions").
///
/// `subaction` selects `plan` (dry-run, default-safe), `exec` (destructive —
/// requires `axon:admin` and `confirm: true`), or the collection-prune
/// convenience operations `dedupe`/`purge`. `target` is either a bare source
/// id or `collection:<name>` for `plan|exec`; `purge` requires a target URL or
/// source key; `dedupe` scans the configured collection.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PruneMcpRequest {
    /// `plan`, `exec`, `dedupe`, or `purge`. Defaults to `plan` when omitted
    /// so a bare `prune` call never mutates state.
    pub subaction: Option<String>,
    /// Prune target: a bare source id, or `collection:<name>` for a
    /// whole-collection prune. **Handler-required despite the `Option`** —
    /// keeps validation errors structured instead of relying on serde's
    /// missing-field rejection.
    pub target: Option<String>,
    /// Narrow a source-id target to one generation. Invalid with a
    /// `collection:` target.
    pub generation: Option<String>,
    pub collection: Option<String>,
    /// Required `Some(true)` for `subaction=exec` to proceed — mirrors the
    /// CLI's `--confirm` gate. A missing/`false` value is treated identically
    /// by the handler; `Option` here is purely to match the doc-generator's
    /// required/optional-field heuristic, not a wire-shape requirement.
    /// Ignored for `subaction=plan`.
    pub confirm: Option<bool>,
    pub response_mode: Option<ResponseMode>,
}

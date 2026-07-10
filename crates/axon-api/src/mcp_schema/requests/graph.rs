//! Graph action request types (issue #298 GQ). Read-only SourceGraph query
//! surface — mirrors the REST `/v1/graph/*` routes
//! (`docs/pipeline-unification/surfaces/rest-contract.md` "Graph Routes",
//! `docs/pipeline-unification/surfaces/tool-contract.md` "Graph subactions").

use serde::{Deserialize, Serialize};

use super::ResponseMode;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphRequest {
    pub subaction: Option<GraphSubaction>,
    /// `node`/`edge`: the id to look up. `resolve`: treated as the identifier
    /// `value` (a stable key) when `node_id`/`canonical_uri` are unset.
    pub id: Option<String>,
    /// `resolve`: explicit `canonical_uri` identifier form.
    pub canonical_uri: Option<String>,
    /// `resolve`: hint for the identifier's expected node kind.
    pub kind: Option<String>,
    /// `query`: start node id (BFS root). Falls back to `id` when unset.
    pub node_id: Option<String>,
    /// `query`: edge-kind allowlist filter (empty = all kinds).
    pub edges: Option<Vec<String>>,
    /// `query`/`source`: traversal direction. Defaults to `both`.
    pub direction: Option<GraphDirectionArg>,
    /// `query`/`source`: max traversal depth. Defaults to `1`.
    pub depth: Option<u32>,
    /// `source`: single edge-kind filter (contract's `edge_kind` field).
    pub edge_kind: Option<String>,
    /// `node`: include incident edges in the response. Defaults to `false`.
    pub include_edges: Option<bool>,
    /// `node`: include edge evidence in the response. Edges already carry
    /// evidence when loaded; this flag is accepted for contract parity and
    /// currently has no additional effect beyond `include_edges`.
    #[allow(dead_code)] // accepted for API compat; evidence always ships with edges
    pub include_evidence: Option<bool>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphSubaction {
    Kinds,
    Resolve,
    Query,
    Node,
    Edge,
    Source,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphDirectionArg {
    In,
    Out,
    Both,
}

use axon_api::source::{ApiError, ErrorStage, GraphQueryRequest};

use super::Result;

pub const DEFAULT_GRAPH_EDGE_LIMIT: u32 = 100;
pub const MAX_GRAPH_EDGE_LIMIT: u32 = 1_000;
pub const MAX_GRAPH_DEPTH: u32 = 8;
pub const MAX_GRAPH_EDGE_KINDS: usize = 64;
pub const MAX_GRAPH_IDENTIFIER_BYTES: usize = 4_096;

pub fn bounded_query(request: &GraphQueryRequest) -> Result<(u32, usize)> {
    if request.edges.len() > MAX_GRAPH_EDGE_KINDS {
        return Err(ApiError::new(
            "graph.edge_filter_limit_exceeded",
            ErrorStage::Retrieving,
            format!("graph edge filters must not exceed {MAX_GRAPH_EDGE_KINDS}"),
        ));
    }
    let identifier_bytes = request.start.kind.len()
        + request.start.canonical_uri.as_deref().map_or(0, str::len)
        + request.start.value.as_deref().map_or(0, str::len)
        + request.start.node_id.as_ref().map_or(0, |id| id.0.len())
        + request.start.source_id.as_ref().map_or(0, |id| id.0.len())
        + request
            .start
            .source_item_key
            .as_ref()
            .map_or(0, |id| id.0.len());
    if identifier_bytes > MAX_GRAPH_IDENTIFIER_BYTES {
        return Err(ApiError::new(
            "graph.identifier_limit_exceeded",
            ErrorStage::Retrieving,
            format!("graph identifier must not exceed {MAX_GRAPH_IDENTIFIER_BYTES} bytes"),
        ));
    }
    bounded_limits(request.depth, request.limit)
}

pub fn bounded_limits(depth: u32, requested_limit: u32) -> Result<(u32, usize)> {
    if depth > MAX_GRAPH_DEPTH {
        return Err(ApiError::new(
            "graph.depth_limit_exceeded",
            ErrorStage::Retrieving,
            format!("graph depth must not exceed {MAX_GRAPH_DEPTH}"),
        ));
    }
    let limit = if requested_limit == 0 {
        DEFAULT_GRAPH_EDGE_LIMIT
    } else {
        requested_limit
    };
    if limit > MAX_GRAPH_EDGE_LIMIT {
        return Err(ApiError::new(
            "graph.edge_limit_exceeded",
            ErrorStage::Retrieving,
            format!("graph edge limit must not exceed {MAX_GRAPH_EDGE_LIMIT}"),
        ));
    }
    Ok((depth, limit as usize))
}

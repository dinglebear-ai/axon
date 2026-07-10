//! REST route registry entries for the read-only `/v1/graph/*` surface
//! (issue #298 WS-D graph query tier).
//!
//! Split out of the parent `schema_registry` module to keep it under the
//! repo's monolith line cap. Spliced into `rest_route_registry()`'s output by
//! the parent module. All graph routes are read-scoped: the graph write path
//! stays parser/source-job owned and has no REST surface.

use super::{RestRouteSpec, read, read_query_surface};

pub(super) static GRAPH_ROUTES: &[RestRouteSpec] = &[
    read("GET", "/v1/graph/kinds", "graph_kinds", "GraphKindDocument"),
    read_query_surface(
        "POST",
        "/v1/graph/resolve",
        "graph_resolve",
        Some("GraphResolveRequest"),
        "GraphResolveResponse",
    ),
    read_query_surface(
        "POST",
        "/v1/graph/query",
        "graph_query",
        Some("GraphQueryRequest"),
        "GraphQueryResponse",
    ),
    read(
        "GET",
        "/v1/graph/nodes/{node_id}",
        "graph_node",
        "GraphNodeDetail",
    ),
    read(
        "GET",
        "/v1/graph/nodes/{node_id}/edges",
        "graph_node_edges",
        "GraphNodeEdges",
    ),
    read(
        "GET",
        "/v1/graph/edges/{edge_id}",
        "graph_edge",
        "GraphEdgeDetail",
    ),
    read(
        "GET",
        "/v1/graph/sources/{source_id}",
        "graph_source_subgraph",
        "GraphSourceSubgraph",
    ),
];

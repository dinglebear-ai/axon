-- axon-graph durable SourceGraph tables.
--
-- Owns graph_nodes, graph_edges, graph_evidence, graph_aliases, and
-- graph_conflicts per the schema contract. Created idempotently
-- (CREATE ... IF NOT EXISTS) so re-running on an existing store is a no-op.
--
-- Provenance columns (source_ids, evidence links, job_id) satisfy the contract
-- requirement that graph evidence always links back to
-- source/item/document/chunk when available.

CREATE TABLE IF NOT EXISTS graph_nodes (
    node_id       TEXT PRIMARY KEY NOT NULL,
    kind          TEXT NOT NULL,
    stable_key    TEXT NOT NULL,
    canonical_uri TEXT NOT NULL,
    display_name  TEXT NOT NULL,
    authority     TEXT NOT NULL,
    confidence    REAL NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    source_ids_json TEXT NOT NULL DEFAULT '[]',
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_nodes_stable_key
    ON graph_nodes (kind, stable_key);

CREATE TABLE IF NOT EXISTS graph_edges (
    edge_id       TEXT PRIMARY KEY NOT NULL,
    kind          TEXT NOT NULL,
    from_node_id  TEXT NOT NULL,
    to_node_id    TEXT NOT NULL,
    authority     TEXT NOT NULL,
    confidence    REAL NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    FOREIGN KEY (from_node_id) REFERENCES graph_nodes (node_id) ON DELETE CASCADE,
    FOREIGN KEY (to_node_id) REFERENCES graph_nodes (node_id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_edges_tuple
    ON graph_edges (kind, from_node_id, to_node_id);

CREATE INDEX IF NOT EXISTS idx_graph_edges_from ON graph_edges (from_node_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_to ON graph_edges (to_node_id);

CREATE TABLE IF NOT EXISTS graph_evidence (
    evidence_id     TEXT NOT NULL,
    edge_id         TEXT NOT NULL,
    evidence_kind   TEXT NOT NULL,
    source_id       TEXT NOT NULL,
    source_item_key TEXT NOT NULL,
    document_id     TEXT,
    chunk_id        TEXT,
    range_json      TEXT,
    quote           TEXT,
    confidence      REAL NOT NULL,
    metadata_json   TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (edge_id, evidence_id),
    FOREIGN KEY (edge_id) REFERENCES graph_edges (edge_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_graph_evidence_source ON graph_evidence (source_id);

CREATE TABLE IF NOT EXISTS graph_aliases (
    alias_kind  TEXT NOT NULL,
    alias_value TEXT NOT NULL,
    node_id     TEXT NOT NULL,
    PRIMARY KEY (alias_kind, alias_value),
    FOREIGN KEY (node_id) REFERENCES graph_nodes (node_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_graph_aliases_node ON graph_aliases (node_id);

CREATE TABLE IF NOT EXISTS graph_conflicts (
    conflict_id   TEXT PRIMARY KEY NOT NULL,
    target_kind   TEXT NOT NULL,
    target_id     TEXT NOT NULL,
    field         TEXT NOT NULL,
    existing_value TEXT NOT NULL,
    incoming_value TEXT NOT NULL,
    existing_authority TEXT NOT NULL,
    incoming_authority TEXT NOT NULL,
    detected_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_graph_conflicts_target
    ON graph_conflicts (target_kind, target_id);

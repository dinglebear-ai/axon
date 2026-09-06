CREATE TABLE IF NOT EXISTS graph_write_checkpoints (
    job_id TEXT NOT NULL,
    candidate_id TEXT NOT NULL,
    next_edge_index INTEGER NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (job_id, candidate_id)
);

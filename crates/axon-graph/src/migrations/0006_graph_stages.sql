CREATE TABLE IF NOT EXISTS graph_stages (
 stage_id TEXT PRIMARY KEY, source_id TEXT NOT NULL, generation_id TEXT NOT NULL,
 job_id TEXT NOT NULL, attempt INTEGER NOT NULL, path TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'building', summary_json TEXT, journal_digest TEXT, write_set_digest TEXT,
 created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS graph_revision (singleton INTEGER PRIMARY KEY CHECK(singleton=1), revision INTEGER NOT NULL);
INSERT OR IGNORE INTO graph_revision VALUES(1,0);
CREATE TRIGGER IF NOT EXISTS graph_revision_nodes_insert AFTER INSERT ON graph_nodes BEGIN UPDATE graph_revision SET revision=revision+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS graph_revision_nodes_update AFTER UPDATE ON graph_nodes BEGIN UPDATE graph_revision SET revision=revision+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS graph_revision_nodes_delete AFTER DELETE ON graph_nodes BEGIN UPDATE graph_revision SET revision=revision+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS graph_revision_edges_insert AFTER INSERT ON graph_edges BEGIN UPDATE graph_revision SET revision=revision+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS graph_revision_edges_update AFTER UPDATE ON graph_edges BEGIN UPDATE graph_revision SET revision=revision+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS graph_revision_edges_delete AFTER DELETE ON graph_edges BEGIN UPDATE graph_revision SET revision=revision+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS graph_revision_evidence_insert AFTER INSERT ON graph_evidence BEGIN UPDATE graph_revision SET revision=revision+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS graph_revision_evidence_update AFTER UPDATE ON graph_evidence BEGIN UPDATE graph_revision SET revision=revision+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS graph_revision_evidence_delete AFTER DELETE ON graph_evidence BEGIN UPDATE graph_revision SET revision=revision+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS graph_revision_aliases_insert AFTER INSERT ON graph_aliases BEGIN UPDATE graph_revision SET revision=revision+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS graph_revision_aliases_update AFTER UPDATE ON graph_aliases BEGIN UPDATE graph_revision SET revision=revision+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS graph_revision_aliases_delete AFTER DELETE ON graph_aliases BEGIN UPDATE graph_revision SET revision=revision+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS graph_revision_conflicts_insert AFTER INSERT ON graph_conflicts BEGIN UPDATE graph_revision SET revision=revision+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS graph_revision_conflicts_update AFTER UPDATE ON graph_conflicts BEGIN UPDATE graph_revision SET revision=revision+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS graph_revision_conflicts_delete AFTER DELETE ON graph_conflicts BEGIN UPDATE graph_revision SET revision=revision+1 WHERE singleton=1; END;
CREATE TABLE IF NOT EXISTS graph_stage_receipts (stage_id TEXT PRIMARY KEY, source_id TEXT NOT NULL, generation_id TEXT NOT NULL, summary_json TEXT NOT NULL, activated_at TEXT NOT NULL DEFAULT (datetime('now')));
CREATE INDEX IF NOT EXISTS idx_graph_stage_receipts_generation ON graph_stage_receipts(source_id,generation_id,activated_at DESC);

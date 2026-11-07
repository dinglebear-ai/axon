-- Retirement identifies owned nodes by stable key across node kinds.
CREATE INDEX IF NOT EXISTS idx_graph_nodes_retirement_key ON graph_nodes (stable_key);

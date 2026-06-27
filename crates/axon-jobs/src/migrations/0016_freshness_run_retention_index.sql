CREATE INDEX IF NOT EXISTS idx_axon_freshness_runs_created
  ON axon_freshness_runs(created_at);

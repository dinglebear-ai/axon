-- Retire the removed `fresh` / `--fresh` scheduler store.
--
-- Recurring acquisition now lives behind SourceRequest-backed watches
-- (`axon_source_watches` / `axon_source_watch_runs`).
DROP TABLE IF EXISTS axon_freshness_runs;
DROP TABLE IF EXISTS axon_freshness_defs;

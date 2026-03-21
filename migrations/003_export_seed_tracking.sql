-- migrations/003_export_seed_tracking.sql
-- Ensure seed-tracking tables exist for search/research and scrape exports.

CREATE TABLE IF NOT EXISTS axon_query_history (
    id BIGSERIAL PRIMARY KEY,
    kind TEXT NOT NULL,
    query_text TEXT NOT NULL,
    options_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_axon_query_history_kind_created_desc
    ON axon_query_history(kind, created_at DESC);

CREATE TABLE IF NOT EXISTS axon_scrape_seeds (
    id BIGSERIAL PRIMARY KEY,
    url TEXT NOT NULL,
    options_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_axon_scrape_seeds_created_desc
    ON axon_scrape_seeds(created_at DESC);

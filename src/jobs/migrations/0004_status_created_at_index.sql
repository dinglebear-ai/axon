-- Composite (status, created_at DESC) index for the CASE-based ORDER BY in
-- list_service_jobs (crawl path). The single-column idx_crawl_status index
-- can satisfy the status filter but forces a temp-sort for the
-- created_at DESC secondary key once axon_crawl_jobs grows past a few
-- thousand rows. Same shape applied to the other three job tables so the
-- same query template stays index-friendly if their list paths later add
-- a CASE-status ORDER BY.

CREATE INDEX IF NOT EXISTS idx_crawl_status_created
    ON axon_crawl_jobs(status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_embed_status_created
    ON axon_embed_jobs(status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_extract_status_created
    ON axon_extract_jobs(status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_ingest_status_created
    ON axon_ingest_jobs(status, created_at DESC);

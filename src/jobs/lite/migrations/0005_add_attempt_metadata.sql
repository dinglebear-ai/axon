ALTER TABLE axon_crawl_jobs ADD COLUMN attempt_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE axon_crawl_jobs ADD COLUMN active_attempt_id TEXT;
ALTER TABLE axon_crawl_jobs ADD COLUMN last_reclaimed_at INTEGER;
ALTER TABLE axon_crawl_jobs ADD COLUMN last_reclaimed_reason TEXT;

ALTER TABLE axon_embed_jobs ADD COLUMN attempt_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE axon_embed_jobs ADD COLUMN active_attempt_id TEXT;
ALTER TABLE axon_embed_jobs ADD COLUMN last_reclaimed_at INTEGER;
ALTER TABLE axon_embed_jobs ADD COLUMN last_reclaimed_reason TEXT;

ALTER TABLE axon_extract_jobs ADD COLUMN attempt_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE axon_extract_jobs ADD COLUMN active_attempt_id TEXT;
ALTER TABLE axon_extract_jobs ADD COLUMN last_reclaimed_at INTEGER;
ALTER TABLE axon_extract_jobs ADD COLUMN last_reclaimed_reason TEXT;

ALTER TABLE axon_ingest_jobs ADD COLUMN attempt_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE axon_ingest_jobs ADD COLUMN active_attempt_id TEXT;
ALTER TABLE axon_ingest_jobs ADD COLUMN last_reclaimed_at INTEGER;
ALTER TABLE axon_ingest_jobs ADD COLUMN last_reclaimed_reason TEXT;

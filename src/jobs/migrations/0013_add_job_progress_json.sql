-- Separate live job progress from terminal job results.
ALTER TABLE axon_crawl_jobs ADD COLUMN progress_json TEXT;
ALTER TABLE axon_embed_jobs ADD COLUMN progress_json TEXT;
ALTER TABLE axon_extract_jobs ADD COLUMN progress_json TEXT;
ALTER TABLE axon_ingest_jobs ADD COLUMN progress_json TEXT;

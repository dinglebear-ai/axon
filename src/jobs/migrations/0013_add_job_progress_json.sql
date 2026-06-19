-- Separate live job progress from terminal job results.
ALTER TABLE axon_crawl_jobs ADD COLUMN progress_json TEXT;
ALTER TABLE axon_embed_jobs ADD COLUMN progress_json TEXT;
ALTER TABLE axon_extract_jobs ADD COLUMN progress_json TEXT;
ALTER TABLE axon_ingest_jobs ADD COLUMN progress_json TEXT;

UPDATE axon_crawl_jobs
SET progress_json = result_json,
    result_json = NULL
WHERE status IN ('pending', 'running') AND progress_json IS NULL AND result_json IS NOT NULL;

UPDATE axon_embed_jobs
SET progress_json = result_json,
    result_json = NULL
WHERE status IN ('pending', 'running') AND progress_json IS NULL AND result_json IS NOT NULL;

UPDATE axon_extract_jobs
SET progress_json = result_json,
    result_json = NULL
WHERE status IN ('pending', 'running') AND progress_json IS NULL AND result_json IS NOT NULL;

UPDATE axon_ingest_jobs
SET progress_json = result_json,
    result_json = NULL
WHERE status IN ('pending', 'running') AND progress_json IS NULL AND result_json IS NOT NULL;

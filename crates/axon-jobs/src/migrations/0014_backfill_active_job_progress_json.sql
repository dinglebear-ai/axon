-- Move active progress snapshots out of terminal result storage.
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

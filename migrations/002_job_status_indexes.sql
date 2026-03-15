-- migrations/002_job_status_indexes.sql
-- Add missing status indexes on job tables for status-filtered API queries.

CREATE INDEX IF NOT EXISTS idx_axon_extract_jobs_status
    ON axon_extract_jobs(status);

CREATE INDEX IF NOT EXISTS idx_axon_embed_jobs_status
    ON axon_embed_jobs(status);

CREATE INDEX IF NOT EXISTS idx_axon_ingest_jobs_status
    ON axon_ingest_jobs(status);

CREATE INDEX IF NOT EXISTS idx_axon_refresh_jobs_status
    ON axon_refresh_jobs(status);

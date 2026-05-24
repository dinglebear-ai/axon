CREATE TABLE IF NOT EXISTS axon_ingest_payloads (
    job_id TEXT PRIMARY KEY NOT NULL,
    payload_kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (job_id) REFERENCES axon_ingest_jobs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_axon_ingest_payloads_kind
    ON axon_ingest_payloads(payload_kind);

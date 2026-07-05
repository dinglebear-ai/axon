CREATE TABLE IF NOT EXISTS axon_job_cutover_receipts (
    receipt_id TEXT PRIMARY KEY NOT NULL,
    receipt_kind TEXT NOT NULL,
    message TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_axon_job_cutover_receipts_kind
    ON axon_job_cutover_receipts(receipt_kind, created_at DESC);

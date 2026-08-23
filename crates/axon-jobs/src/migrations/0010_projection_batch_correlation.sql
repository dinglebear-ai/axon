CREATE TABLE projection_batch_items (
    batch_id TEXT NOT NULL,
    item_index INTEGER NOT NULL CHECK (item_index >= 0),
    job_id TEXT NOT NULL REFERENCES jobs(job_id) ON DELETE CASCADE,
    operation TEXT NOT NULL CHECK (operation IN ('scrape', 'crawl', 'embed', 'ingest')),
    reused INTEGER NOT NULL CHECK (reused IN (0, 1)),
    principal_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (batch_id, item_index),
    UNIQUE (batch_id, job_id, item_index)
);

CREATE INDEX idx_projection_batch_items_principal_batch
    ON projection_batch_items(principal_id, batch_id, item_index);

CREATE INDEX idx_projection_batch_items_job
    ON projection_batch_items(job_id);

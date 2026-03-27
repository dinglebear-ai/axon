CREATE TABLE IF NOT EXISTS axon_crawl_jobs_v2 (
    id          TEXT PRIMARY KEY,
    status      TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed', 'canceled')),
    url         TEXT NOT NULL DEFAULT '',
    config_json TEXT NOT NULL DEFAULT '{}',
    result_json TEXT,
    error_text  TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    started_at  INTEGER,
    finished_at INTEGER
);
INSERT INTO axon_crawl_jobs_v2
SELECT * FROM axon_crawl_jobs
WHERE status IN ('pending', 'running', 'completed', 'failed', 'canceled');
DROP TABLE axon_crawl_jobs;
ALTER TABLE axon_crawl_jobs_v2 RENAME TO axon_crawl_jobs;
CREATE INDEX IF NOT EXISTS idx_crawl_status ON axon_crawl_jobs(status);

CREATE TABLE IF NOT EXISTS axon_embed_jobs_v2 (
    id          TEXT PRIMARY KEY,
    status      TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed', 'canceled')),
    input_text  TEXT NOT NULL DEFAULT '',
    config_json TEXT NOT NULL DEFAULT '{}',
    result_json TEXT,
    error_text  TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    started_at  INTEGER,
    finished_at INTEGER
);
INSERT INTO axon_embed_jobs_v2
SELECT * FROM axon_embed_jobs
WHERE status IN ('pending', 'running', 'completed', 'failed', 'canceled');
DROP TABLE axon_embed_jobs;
ALTER TABLE axon_embed_jobs_v2 RENAME TO axon_embed_jobs;
CREATE INDEX IF NOT EXISTS idx_embed_status ON axon_embed_jobs(status);

CREATE TABLE IF NOT EXISTS axon_extract_jobs_v2 (
    id          TEXT PRIMARY KEY,
    status      TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed', 'canceled')),
    urls_json   TEXT NOT NULL DEFAULT '[]',
    config_json TEXT NOT NULL DEFAULT '{}',
    result_json TEXT,
    error_text  TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    started_at  INTEGER,
    finished_at INTEGER
);
INSERT INTO axon_extract_jobs_v2
SELECT * FROM axon_extract_jobs
WHERE status IN ('pending', 'running', 'completed', 'failed', 'canceled');
DROP TABLE axon_extract_jobs;
ALTER TABLE axon_extract_jobs_v2 RENAME TO axon_extract_jobs;
CREATE INDEX IF NOT EXISTS idx_extract_status ON axon_extract_jobs(status);

CREATE TABLE IF NOT EXISTS axon_ingest_jobs_v2 (
    id          TEXT PRIMARY KEY,
    status      TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed', 'canceled')),
    source_type TEXT NOT NULL DEFAULT '',
    target      TEXT NOT NULL DEFAULT '',
    config_json TEXT NOT NULL DEFAULT '{}',
    result_json TEXT,
    error_text  TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    started_at  INTEGER,
    finished_at INTEGER
);
INSERT INTO axon_ingest_jobs_v2
SELECT * FROM axon_ingest_jobs
WHERE status IN ('pending', 'running', 'completed', 'failed', 'canceled');
DROP TABLE axon_ingest_jobs;
ALTER TABLE axon_ingest_jobs_v2 RENAME TO axon_ingest_jobs;
CREATE INDEX IF NOT EXISTS idx_ingest_status ON axon_ingest_jobs(status);

CREATE TABLE IF NOT EXISTS axon_refresh_jobs_v2 (
    id          TEXT PRIMARY KEY,
    status      TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed', 'canceled')),
    url         TEXT NOT NULL DEFAULT '',
    config_json TEXT NOT NULL DEFAULT '{}',
    result_json TEXT,
    error_text  TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    started_at  INTEGER,
    finished_at INTEGER
);
INSERT INTO axon_refresh_jobs_v2
SELECT * FROM axon_refresh_jobs
WHERE status IN ('pending', 'running', 'completed', 'failed', 'canceled');
DROP TABLE axon_refresh_jobs;
ALTER TABLE axon_refresh_jobs_v2 RENAME TO axon_refresh_jobs;
CREATE INDEX IF NOT EXISTS idx_refresh_status ON axon_refresh_jobs(status);

CREATE TABLE IF NOT EXISTS axon_graph_jobs_v2 (
    id          TEXT PRIMARY KEY,
    status      TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed', 'canceled')),
    config_json TEXT NOT NULL DEFAULT '{}',
    result_json TEXT,
    error_text  TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    started_at  INTEGER,
    finished_at INTEGER
);
INSERT INTO axon_graph_jobs_v2
SELECT * FROM axon_graph_jobs
WHERE status IN ('pending', 'running', 'completed', 'failed', 'canceled');
DROP TABLE axon_graph_jobs;
ALTER TABLE axon_graph_jobs_v2 RENAME TO axon_graph_jobs;
CREATE INDEX IF NOT EXISTS idx_graph_status ON axon_graph_jobs(status);

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
INSERT INTO axon_crawl_jobs_v2 (id, status, url, config_json, result_json, error_text, created_at, updated_at, started_at, finished_at)
SELECT id,
       CASE WHEN status IN ('pending','running','completed','failed','canceled') THEN status ELSE 'failed' END,
       url, config_json, result_json,
       CASE WHEN status NOT IN ('pending','running','completed','failed','canceled')
            THEN 'unknown status at migration 0003: ' || COALESCE(status, 'NULL')
            ELSE error_text END,
       created_at, updated_at, started_at, finished_at
FROM axon_crawl_jobs;
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
INSERT INTO axon_embed_jobs_v2 (id, status, input_text, config_json, result_json, error_text, created_at, updated_at, started_at, finished_at)
SELECT id,
       CASE WHEN status IN ('pending','running','completed','failed','canceled') THEN status ELSE 'failed' END,
       input_text, config_json, result_json,
       CASE WHEN status NOT IN ('pending','running','completed','failed','canceled')
            THEN 'unknown status at migration 0003: ' || COALESCE(status, 'NULL')
            ELSE error_text END,
       created_at, updated_at, started_at, finished_at
FROM axon_embed_jobs;
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
INSERT INTO axon_extract_jobs_v2 (id, status, urls_json, config_json, result_json, error_text, created_at, updated_at, started_at, finished_at)
SELECT id,
       CASE WHEN status IN ('pending','running','completed','failed','canceled') THEN status ELSE 'failed' END,
       urls_json, config_json, result_json,
       CASE WHEN status NOT IN ('pending','running','completed','failed','canceled')
            THEN 'unknown status at migration 0003: ' || COALESCE(status, 'NULL')
            ELSE error_text END,
       created_at, updated_at, started_at, finished_at
FROM axon_extract_jobs;
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
INSERT INTO axon_ingest_jobs_v2 (id, status, source_type, target, config_json, result_json, error_text, created_at, updated_at, started_at, finished_at)
SELECT id,
       CASE WHEN status IN ('pending','running','completed','failed','canceled') THEN status ELSE 'failed' END,
       source_type, target, config_json, result_json,
       CASE WHEN status NOT IN ('pending','running','completed','failed','canceled')
            THEN 'unknown status at migration 0003: ' || COALESCE(status, 'NULL')
            ELSE error_text END,
       created_at, updated_at, started_at, finished_at
FROM axon_ingest_jobs;
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
INSERT INTO axon_refresh_jobs_v2 (id, status, url, config_json, result_json, error_text, created_at, updated_at, started_at, finished_at)
SELECT id,
       CASE WHEN status IN ('pending','running','completed','failed','canceled') THEN status ELSE 'failed' END,
       url, config_json, result_json,
       CASE WHEN status NOT IN ('pending','running','completed','failed','canceled')
            THEN 'unknown status at migration 0003: ' || COALESCE(status, 'NULL')
            ELSE error_text END,
       created_at, updated_at, started_at, finished_at
FROM axon_refresh_jobs;
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
INSERT INTO axon_graph_jobs_v2 (id, status, config_json, result_json, error_text, created_at, updated_at, started_at, finished_at)
SELECT id,
       CASE WHEN status IN ('pending','running','completed','failed','canceled') THEN status ELSE 'failed' END,
       config_json, result_json,
       CASE WHEN status NOT IN ('pending','running','completed','failed','canceled')
            THEN 'unknown status at migration 0003: ' || COALESCE(status, 'NULL')
            ELSE error_text END,
       created_at, updated_at, started_at, finished_at
FROM axon_graph_jobs;
DROP TABLE axon_graph_jobs;
ALTER TABLE axon_graph_jobs_v2 RENAME TO axon_graph_jobs;
CREATE INDEX IF NOT EXISTS idx_graph_status ON axon_graph_jobs(status);

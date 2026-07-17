CREATE TABLE axon_applied_migrations (
    namespace TEXT NOT NULL,
    version INTEGER NOT NULL,
    name TEXT NOT NULL,
    applied_at TEXT NOT NULL,
    PRIMARY KEY (namespace, version)
);
INSERT INTO axon_applied_migrations (namespace, version, name, applied_at)
VALUES ('jobs', 1, '0001_canonical_jobs', '2026-07-01T00:00:00Z');
CREATE TABLE jobs (job_id TEXT PRIMARY KEY);

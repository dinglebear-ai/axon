CREATE TABLE IF NOT EXISTS axon_watch_url_state (
    watch_id          TEXT NOT NULL,
    url               TEXT NOT NULL,
    etag              TEXT,
    last_modified     TEXT,
    content_hash      TEXT,
    last_markdown     TEXT,
    last_links_json   TEXT,
    last_checked_at   INTEGER,
    last_changed_at   INTEGER,
    last_crawl_job_id TEXT,
    PRIMARY KEY (watch_id, url),
    FOREIGN KEY (watch_id) REFERENCES axon_watch_defs(id) ON DELETE CASCADE
);

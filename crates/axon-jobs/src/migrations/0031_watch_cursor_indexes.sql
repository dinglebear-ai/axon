-- Keyset pagination for canonical source watches and linked run history.
CREATE INDEX IF NOT EXISTS idx_source_watches_created_cursor
    ON axon_source_watches(created_at DESC, watch_id DESC);

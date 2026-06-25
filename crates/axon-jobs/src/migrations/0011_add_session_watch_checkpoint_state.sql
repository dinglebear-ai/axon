ALTER TABLE axon_session_watch_checkpoints
    ADD COLUMN state TEXT NOT NULL DEFAULT 'pending';

ALTER TABLE axon_session_watch_checkpoints
    ADD COLUMN remote_job_id TEXT;

CREATE INDEX IF NOT EXISTS idx_axon_session_watch_checkpoints_state
    ON axon_session_watch_checkpoints(state);

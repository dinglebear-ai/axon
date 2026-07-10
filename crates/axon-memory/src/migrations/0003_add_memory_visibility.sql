-- Add the `visibility` security classification column (contract "Security
-- and Redaction": "classify every memory by visibility"). Existing rows
-- (written before this column existed) default to `internal`, matching
-- `Visibility::default()` in axon-api.

ALTER TABLE memory_records ADD COLUMN visibility TEXT NOT NULL DEFAULT 'internal';

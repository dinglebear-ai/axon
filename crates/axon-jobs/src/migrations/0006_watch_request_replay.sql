ALTER TABLE axon_source_watches ADD COLUMN limits_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE axon_source_watches ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}';

ALTER TABLE axon_source_watches
    ADD COLUMN limits_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(limits_json));
ALTER TABLE axon_source_watches
    ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json));

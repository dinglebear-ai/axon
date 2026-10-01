-- Narrow retirement to one item's evidence, including baseline contained-item provenance.
CREATE INDEX IF NOT EXISTS idx_graph_evidence_item
ON graph_evidence (source_id, coalesce(json_extract(metadata_json, '$.contained_source_item_key'), source_item_key));

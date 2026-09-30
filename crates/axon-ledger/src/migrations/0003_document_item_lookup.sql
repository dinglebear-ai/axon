CREATE INDEX IF NOT EXISTS idx_document_status_source_item
  ON document_status(source_id, source_item_key);

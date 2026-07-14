/// One bounded batch boundary in the source pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePipelineBatch {
    pub batch_id: usize,
    pub item_count: usize,
    pub chunk_count: usize,
    pub byte_count: usize,
    pub provider_reservation_id: Option<String>,
    pub elapsed_ms: u64,
}

/// Build the canonical bounded batch plan shared by source-family ports.
///
/// Source adapters stream item/document candidates. The service layer applies
/// this boundary before prepare, embedding, vector upsert, and graph writes so
/// no public source path needs to collect the whole source before downstream
/// stages can make progress.
pub fn plan_source_pipeline_batches(
    item_count: usize,
    batch_size: usize,
) -> anyhow::Result<Vec<SourcePipelineBatch>> {
    if batch_size == 0 {
        anyhow::bail!("source pipeline batch size must be greater than zero");
    }

    Ok((0..item_count)
        .collect::<Vec<_>>()
        .chunks(batch_size)
        .enumerate()
        .map(|(batch_id, chunk)| SourcePipelineBatch {
            batch_id,
            item_count: chunk.len(),
            chunk_count: chunk.len(),
            byte_count: 0,
            provider_reservation_id: None,
            elapsed_ms: 0,
        })
        .collect())
}

#[cfg(test)]
#[path = "../source_batch_tests.rs"]
mod source_batch_tests;

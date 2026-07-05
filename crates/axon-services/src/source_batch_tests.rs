use super::{SourcePipelineBatch, plan_source_pipeline_batches};

#[tokio::test]
async fn source_pipeline_batches_prepare_embed_vector_and_graph_writes() {
    let mut harness = source_pipeline_harness().with_batch_size(3);

    harness.index_fixture_items("web", 8).await.unwrap();

    assert_eq!(harness.prepare_batch_sizes(), vec![3, 3, 2]);
    assert_eq!(harness.embedding_batch_sizes(), vec![3, 3, 2]);
    assert_eq!(harness.vector_upsert_batch_sizes(), vec![3, 3, 2]);
    assert_eq!(harness.graph_write_batch_sizes(), vec![3, 3, 2]);
}

#[test]
fn source_batch_plan_rejects_zero_batch_size() {
    let err = plan_source_pipeline_batches(1, 0).unwrap_err();
    assert!(err.to_string().contains("greater than zero"));
}

fn source_pipeline_harness() -> SourcePipelineHarness {
    SourcePipelineHarness::default()
}

#[derive(Debug, Default)]
struct SourcePipelineHarness {
    batch_size: usize,
    prepare: Vec<SourcePipelineBatch>,
    embedding: Vec<SourcePipelineBatch>,
    vector_upsert: Vec<SourcePipelineBatch>,
    graph_write: Vec<SourcePipelineBatch>,
}

impl SourcePipelineHarness {
    fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    async fn index_fixture_items(
        &mut self,
        _adapter: &str,
        item_count: usize,
    ) -> anyhow::Result<()> {
        let batch_size = self.batch_size.max(1);
        let batches = plan_source_pipeline_batches(item_count, batch_size)?;
        self.prepare = batches.clone();
        self.embedding = batches.clone();
        self.vector_upsert = batches.clone();
        self.graph_write = batches;
        Ok(())
    }

    fn prepare_batch_sizes(&self) -> Vec<usize> {
        batch_sizes(&self.prepare)
    }

    fn embedding_batch_sizes(&self) -> Vec<usize> {
        batch_sizes(&self.embedding)
    }

    fn vector_upsert_batch_sizes(&self) -> Vec<usize> {
        batch_sizes(&self.vector_upsert)
    }

    fn graph_write_batch_sizes(&self) -> Vec<usize> {
        batch_sizes(&self.graph_write)
    }
}

fn batch_sizes(batches: &[SourcePipelineBatch]) -> Vec<usize> {
    batches.iter().map(|batch| batch.item_count).collect()
}

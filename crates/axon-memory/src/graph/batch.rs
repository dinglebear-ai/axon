use super::*;

impl GraphBackedMemoryStore {
    pub(super) async fn mark_graph_recovery(
        &self,
        records: &[MemoryRecord],
        error: &ApiError,
        warnings: &mut Vec<SourceWarning>,
    ) -> Result<()> {
        for record in records {
            self.inner
                .set_status(MemoryStatusRequest {
                    memory_id: record.memory_id.clone(),
                    status: MemoryStatus::Review,
                    reason: Some(format!("memory.graph_failed: {}", error.message)),
                    timestamp: Timestamp::from(chrono::Utc::now()),
                })
                .await?;
            warnings.push(graph_recovery_warning(&record.memory_id, error));
        }
        Ok(())
    }
}

fn graph_recovery_warning(memory_id: &MemoryId, error: &ApiError) -> SourceWarning {
    SourceWarning {
        code: "memory.graph_failed".to_string(),
        severity: Severity::Warning,
        message: format!(
            "memory {} is durable but its graph mirror failed: {}; retry a memory update or review action to repair it",
            memory_id.0, error.message
        ),
        source_item_key: None,
        retryable: true,
    }
}

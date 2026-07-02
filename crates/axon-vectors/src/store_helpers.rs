use axon_api::source::*;

pub(crate) fn delete_result(collection: String, points_deleted: u64) -> VectorStoreDeleteResult {
    VectorStoreDeleteResult {
        collection,
        points_matched: points_deleted,
        points_deleted,
        dry_run: false,
        warnings: Vec::new(),
        metadata: MetadataMap::new(),
    }
}

pub(crate) fn dot_score(left: &[f32], right: &[f32]) -> f64 {
    left.iter()
        .zip(right.iter())
        .map(|(left, right)| f64::from(*left) * f64::from(*right))
        .sum()
}

pub(crate) fn sparse_dot_score(query: Option<&SparseVector>, point: Option<&SparseVector>) -> f64 {
    let (Some(query), Some(point)) = (query, point) else {
        return 0.0;
    };
    query
        .indices
        .iter()
        .zip(query.values.iter())
        .map(|(query_index, query_value)| {
            point
                .indices
                .iter()
                .position(|point_index| point_index == query_index)
                .and_then(|position| point.values.get(position))
                .map(|point_value| f64::from(*query_value) * f64::from(*point_value))
                .unwrap_or(0.0)
        })
        .sum()
}

pub(crate) fn payload_string(payload: &MetadataMap, field: &str) -> Option<String> {
    payload.get(field)?.as_str().map(ToString::to_string)
}

pub(crate) fn stage_header(phase: PipelinePhase) -> StageResultHeader {
    let timestamp = Timestamp("2026-07-01T00:00:00Z".to_string());
    StageResultHeader {
        job_id: JobId::new(uuid::Uuid::from_u128(0)),
        stage_id: StageId::new(uuid::Uuid::from_u128(0)),
        phase,
        status: LifecycleStatus::Completed,
        started_at: timestamp.clone(),
        completed_at: Some(timestamp),
        counts: StageCounts {
            items_total: None,
            items_done: 0,
            documents_total: None,
            documents_done: 0,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        warnings: Vec::new(),
        error: None,
    }
}

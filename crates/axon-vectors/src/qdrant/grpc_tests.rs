use super::*;
use qdrant_client::qdrant::{PointStruct, point_id::PointIdOptions};
use std::sync::{Arc, Mutex};

struct FakeUpserter {
    waits: Mutex<Vec<Option<bool>>>,
    fail_wait: Option<bool>,
}

#[async_trait::async_trait]
impl GrpcUpserter for FakeUpserter {
    async fn upsert(&self, request: UpsertPoints) -> std::result::Result<(), String> {
        self.waits.lock().unwrap().push(request.wait);
        if request.wait == self.fail_wait.map(Some).unwrap_or(None) {
            return Err("injected".to_string());
        }
        Ok(())
    }
}

fn point(id: u64) -> PointStruct {
    PointStruct::new(id, vec![id as f32], qdrant_client::Payload::default())
}

#[test]
fn async_grpc_plan_keeps_completion_fence_separate_from_parallel_chunks() {
    let final_point = point(4);
    let (requests, barrier) = grpc_upsert_plan(
        "vectors",
        vec![
            vec![point(1), point(2)],
            vec![point(3), final_point.clone()],
        ],
        true,
    );
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests
            .iter()
            .map(|request| request.points.len())
            .collect::<Vec<_>>(),
        vec![2, 2]
    );
    assert!(requests.iter().all(|request| request.wait == Some(false)));
    assert!(requests.iter().all(|request| {
        request
            .ordering
            .as_ref()
            .is_some_and(|ordering| ordering.r#type == WriteOrderingType::Strong as i32)
    }));
    let ids = requests[0]
        .points
        .iter()
        .map(|point| {
            match point
                .id
                .as_ref()
                .and_then(|id| id.point_id_options.as_ref())
            {
                Some(PointIdOptions::Num(value)) => *value,
                _ => panic!("numeric point id"),
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, vec![1, 2]);
    let barrier = barrier.expect("async plan has completion fence");
    assert_eq!(barrier.wait, Some(true));
    assert_eq!(barrier.points.len(), 1);
    assert_eq!(barrier.points[0], final_point);
}

#[test]
fn synchronous_grpc_plan_waits_each_chunk_without_extra_fence() {
    let (requests, barrier) =
        grpc_upsert_plan("vectors", vec![vec![point(1)], vec![point(2)]], false);
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|request| request.wait == Some(true)));
    assert!(barrier.is_none());
}

#[tokio::test]
async fn grpc_executor_drains_async_writes_before_fence_and_counts_requests() {
    let (writes, barrier) = grpc_upsert_plan("vectors", vec![vec![point(1)], vec![point(2)]], true);
    let client = Arc::new(FakeUpserter {
        waits: Mutex::new(Vec::new()),
        fail_wait: None,
    });
    let count = execute_grpc_plan(client.clone(), writes, barrier, 2)
        .await
        .expect("execute");
    assert_eq!(count, 3);
    let waits = client.waits.lock().unwrap();
    assert_eq!(waits.iter().filter(|wait| **wait == Some(false)).count(), 2);
    assert_eq!(waits.last(), Some(&Some(true)));
}

#[tokio::test]
async fn grpc_executor_classifies_write_and_fence_failures() {
    let (writes, barrier) = grpc_upsert_plan("vectors", vec![vec![point(1)]], true);
    let write_failure = Arc::new(FakeUpserter {
        waits: Mutex::new(Vec::new()),
        fail_wait: Some(false),
    });
    let error = execute_grpc_plan(write_failure, writes.clone(), barrier.clone(), 1)
        .await
        .unwrap_err();
    assert_eq!(error.code.0, "vector.qdrant.grpc_upsert");
    let fence_failure = Arc::new(FakeUpserter {
        waits: Mutex::new(Vec::new()),
        fail_wait: Some(true),
    });
    let error = execute_grpc_plan(fence_failure, writes, barrier, 1)
        .await
        .unwrap_err();
    assert_eq!(error.code.0, "vector.qdrant.grpc_barrier");
}

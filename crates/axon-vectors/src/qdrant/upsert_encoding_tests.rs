//! Wire equivalence and repeatable CPU measurement for single-pass encoding.
use super::*;
use std::time::Instant;

fn fixture() -> (CollectionSpec, VectorPointBatch) {
    let mut spec = tests::test_collection_spec();
    spec.dense.dimensions = 1024;
    let mut batch = tests::batch(512);
    batch.dimensions = 1024;
    for point in &mut batch.points {
        point.vector = (0..1024).map(|n| (n as f32 - 512.0) / 1024.0).collect();
        point.payload.insert(
            "text".into(),
            serde_json::json!("Unicode λ and escaped \"text\"\n".repeat(24)),
        );
    }
    (spec, batch)
}

#[test]
fn capped_single_pass_matches_dense_sparse_wire_at_exact_limit() {
    let (spec, chunk) = fixture();
    let sparse = chunk
        .sparse_vectors
        .as_ref()
        .unwrap()
        .iter()
        .map(|v| (v.chunk_id.0.as_str(), v))
        .collect::<HashMap<_, _>>();
    let body = UpsertPointsBody::new(&spec, &chunk.points, &sparse);
    let expected = serde_json::to_vec(&body).unwrap();
    assert!(expected.len() < MAX_UPSERT_REQUEST_BYTES);
    let actual = encode_upsert_body(&body, expected.len(), ErrorStage::Upserting)
        .unwrap()
        .unwrap();
    assert_eq!(actual, expected);
    assert!(
        encode_upsert_body(&body, expected.len() - 1, ErrorStage::Upserting)
            .unwrap()
            .is_none()
    );
    let ranges = byte_bounded_ranges(
        &chunk,
        &spec,
        &sparse,
        MAX_UPSERT_REQUEST_BYTES,
        ErrorStage::Upserting,
    )
    .unwrap();
    assert_eq!(ranges, vec![0..chunk.points.len()]);
}

#[test]
#[ignore = "manual CPU benchmark; no timing threshold in CI"]
fn measure_single_pass_encoding_against_singleton_size_scan() {
    let (spec, chunk) = fixture();
    let sparse = chunk
        .sparse_vectors
        .as_ref()
        .unwrap()
        .iter()
        .map(|v| (v.chunk_id.0.as_str(), v))
        .collect::<HashMap<_, _>>();
    let mut totals = [0_u128; 2];
    // Alternate order to reduce warmup/order bias. Measure identical output.
    for iteration in 0..20 {
        for mode in [iteration % 2, (iteration + 1) % 2] {
            let start = Instant::now();
            let bytes = if mode == 0 {
                let ranges = byte_bounded_ranges(
                    &chunk,
                    &spec,
                    &sparse,
                    MAX_UPSERT_REQUEST_BYTES,
                    ErrorStage::Upserting,
                )
                .unwrap();
                assert_eq!(ranges.len(), 1);
                encode_upsert_body(
                    &UpsertPointsBody::new(&spec, &chunk.points[ranges[0].clone()], &sparse),
                    MAX_UPSERT_REQUEST_BYTES,
                    ErrorStage::Upserting,
                )
                .unwrap()
                .unwrap()
            } else {
                encode_upsert_body(
                    &UpsertPointsBody::new(&spec, &chunk.points, &sparse),
                    MAX_UPSERT_REQUEST_BYTES,
                    ErrorStage::Upserting,
                )
                .unwrap()
                .unwrap()
            };
            std::hint::black_box(bytes);
            totals[mode] += start.elapsed().as_micros();
        }
    }
    eprintln!(
        "encoding benchmark: 20 x 512 points x 1024 dimensions; singleton_scan_us={} single_pass_us={} ratio={:.3}",
        totals[0],
        totals[1],
        totals[0] as f64 / totals[1] as f64
    );
}

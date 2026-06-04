use super::*;
use crate::jobs::backend::JobKind;
use uuid::Uuid;

#[test]
fn task_id_round_trips_supported_job_kinds() {
    let id = Uuid::new_v4();
    for kind in [
        JobKind::Crawl,
        JobKind::Embed,
        JobKind::Extract,
        JobKind::Ingest,
    ] {
        let task_id = task_id_for(kind, id);
        assert_eq!(parse_task_id(&task_id).unwrap(), (kind, id));
    }
}

#[test]
fn parse_task_id_rejects_malformed_ids() {
    let cases = [
        "notaxon:crawl:550e8400-e29b-41d4-a716-446655440000",
        "axon",
        "axon:unknown:550e8400-e29b-41d4-a716-446655440000",
        "axon:crawl:not-a-uuid",
        "axon:crawl:550e8400-e29b-41d4-a716-446655440000:extra",
    ];
    for case in cases {
        assert!(parse_task_id(case).is_err(), "{case} should fail");
    }
}

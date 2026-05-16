use super::JobStatus;
use crate::jobs::embed::EmbedJob;
use chrono::{DateTime, TimeZone, Utc};
use std::error::Error;
use uuid::Uuid;

fn test_ts() -> Result<DateTime<Utc>, Box<dyn Error>> {
    Utc.with_ymd_and_hms(2026, 3, 15, 12, 0, 0)
        .single()
        .ok_or_else(|| "valid timestamp".into())
}

fn assert_job_status_trait<T: JobStatus>(
    job: &T,
    expected_status: &str,
) -> Result<(), Box<dyn Error>> {
    assert_eq!(job.status(), expected_status);
    assert_eq!(job.updated_at(), test_ts()?);
    Ok(())
}

#[test]
fn embed_job_implements_shared_job_status_trait() -> Result<(), Box<dyn Error>> {
    let job = EmbedJob {
        id: Uuid::parse_str("66666666-6666-6666-6666-666666666666")?,
        status: "running".to_string(),
        created_at: test_ts()?,
        updated_at: test_ts()?,
        started_at: Some(test_ts()?),
        finished_at: None,
        error_text: None,
        input_text: "/tmp/embed-input".to_string(),
        result_json: Some(serde_json::json!({"chunks_embedded": 3})),
        config_json: serde_json::json!({"collection": "cortex"}),
    };

    assert_job_status_trait(&job, "running")
}

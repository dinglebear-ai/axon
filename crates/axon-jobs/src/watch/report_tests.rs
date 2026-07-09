use super::*;
use crate::store::open_sqlite_pool;
use crate::watch::{WatchDefCreate, create_watch_def_with_pool, create_watch_run_with_pool};
use axon_api::diff::{DiffResult, DiffStatus, LinkEntry};
use chrono::Utc;
use tempfile::NamedTempFile;

fn sample_diff() -> DiffResult {
    DiffResult {
        url_a: "u".into(),
        url_b: "u".into(),
        status: DiffStatus::Changed,
        text_diff: Some("@@\n-old\n+new\n".into()),
        metadata_changes: vec![],
        links_added: vec![LinkEntry {
            href: "h".into(),
            text: "t".into(),
        }],
        links_removed: vec![],
        word_count_delta: 3,
    }
}

#[test]
fn prompt_includes_diff_and_url() {
    let p = summary_user_prompt("https://e/a", &sample_diff());
    assert!(p.contains("https://e/a"));
    assert!(p.contains("+new"));
}

#[tokio::test]
async fn writes_one_change_artifact() {
    let temp = NamedTempFile::new().unwrap();
    let pool = open_sqlite_pool(&temp.path().to_string_lossy())
        .await
        .unwrap();
    let watch = create_watch_def_with_pool(
        &pool,
        &WatchDefCreate {
            name: "w".into(),
            task_type: "watch".into(),
            task_payload: serde_json::json!({"urls":["https://e/a"]}),
            every_seconds: 60,
            enabled: true,
            next_run_at: Utc::now(),
        },
    )
    .await
    .unwrap();
    let run = create_watch_run_with_pool(&pool, watch.id, None)
        .await
        .unwrap();

    write_change_artifact(
        &pool,
        run.id,
        "https://e/a",
        &sample_diff(),
        Some("summary".into()),
    )
    .await
    .unwrap();

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM axon_watch_run_artifacts WHERE watch_run_id = ? AND kind = 'url-change'",
    )
    .bind(run.id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn secret_in_summary_is_redacted_before_persisting() {
    // Fail-closed: a change summary carrying a leaked secret (e.g. the LLM
    // summarizer echoing an auth header found in a page diff) must not reach
    // the `axon_watch_run_artifacts` row unredacted.
    let temp = NamedTempFile::new().unwrap();
    let pool = open_sqlite_pool(&temp.path().to_string_lossy())
        .await
        .unwrap();
    let watch = create_watch_def_with_pool(
        &pool,
        &WatchDefCreate {
            name: "w".into(),
            task_type: "watch".into(),
            task_payload: serde_json::json!({"urls":["https://e/a"]}),
            every_seconds: 60,
            enabled: true,
            next_run_at: Utc::now(),
        },
    )
    .await
    .unwrap();
    let run = create_watch_run_with_pool(&pool, watch.id, None)
        .await
        .unwrap();

    let secret_summary = "rotated authorization: bearer abcdef0123456789abcdef";
    write_change_artifact(
        &pool,
        run.id,
        "https://e/a",
        &sample_diff(),
        Some(secret_summary.to_string()),
    )
    .await
    .unwrap();

    let payload: String = sqlx::query_scalar(
        "SELECT payload FROM axon_watch_run_artifacts WHERE watch_run_id = ? AND kind = 'url-change'",
    )
    .bind(run.id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        !payload.contains("abcdef0123456789abcdef"),
        "secret leaked into persisted artifact payload: {payload}"
    );
    assert!(payload.contains(axon_core::redact::REDACTION_PLACEHOLDER));
}

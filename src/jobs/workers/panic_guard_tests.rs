use super::*;

// NOTE: the intentional panics below print a short backtrace line to stderr via
// the default panic hook — that is expected. We do not swap the global hook
// because it is process-wide and cargo runs these tests in parallel.

// The trailing `Ok(None)` gives each panicking future a concrete `JobResult`
// output type — `async { panic!() }` alone has output `!`, which will not unify
// with the `Output = JobResult` bound as an associated type.
#[tokio::test]
async fn panicking_runner_becomes_job_failure_not_task_death() {
    let result = run_catching(
        async {
            panic!("boom in runner");
            #[allow(unreachable_code)]
            Ok(None)
        },
        JobKind::Crawl,
        Uuid::nil(),
    )
    .await;
    let err = result.expect_err("a panic must be converted into Err, not propagated");
    let msg = err.to_string();
    assert!(msg.contains("job panicked"), "got: {msg}");
    assert!(msg.contains("boom in runner"), "got: {msg}");
}

#[tokio::test]
async fn string_panic_payload_is_captured() {
    let result = run_catching(
        async {
            panic!("{}", String::from("dynamic message"));
            #[allow(unreachable_code)]
            Ok(None)
        },
        JobKind::Embed,
        Uuid::nil(),
    )
    .await;
    let msg = result.expect_err("panic -> Err").to_string();
    assert!(msg.contains("dynamic message"), "got: {msg}");
}

#[tokio::test]
async fn successful_runner_result_passes_through_unchanged() {
    let payload = serde_json::json!({"ok": true});
    let expected = payload.clone();
    let result = run_catching(
        async move { Ok(Some(payload)) },
        JobKind::Crawl,
        Uuid::nil(),
    )
    .await;
    let value = result
        .expect("ok result passes through")
        .expect("some payload");
    assert_eq!(value, expected);
}

#[tokio::test]
async fn error_runner_result_passes_through_unchanged() {
    let result = run_catching(
        async { Err("ordinary failure".into()) },
        JobKind::Crawl,
        Uuid::nil(),
    )
    .await;
    let msg = result.expect_err("err passes through").to_string();
    assert_eq!(msg, "ordinary failure");
    // An ordinary error must NOT be relabeled as a panic.
    assert!(!msg.contains("job panicked"), "got: {msg}");
}

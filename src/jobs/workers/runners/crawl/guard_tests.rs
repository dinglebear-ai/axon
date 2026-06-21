use super::*;

use std::time::Duration;

#[tokio::test]
async fn completed_when_engine_finishes_first() {
    let mut engine = Box::pin(async { 42i32 });
    assert!(matches!(
        race_engine_guards(&mut engine, None, None).await,
        GuardOutcome::Completed(42)
    ));
}

#[tokio::test]
async fn runs_unbounded_without_cancel_or_deadline() {
    // No cancel token and no deadline => both guard arms wait forever, so the
    // engine future drives to completion.
    let mut engine = Box::pin(async {
        tokio::task::yield_now().await;
        99i32
    });
    assert!(matches!(
        race_engine_guards(&mut engine, None, None).await,
        GuardOutcome::Completed(99)
    ));
}

#[tokio::test]
async fn times_out_and_leaves_engine_drainable() {
    let mut engine = Box::pin(async {
        tokio::time::sleep(Duration::from_secs(60)).await;
        7i32
    });
    let deadline = tokio::time::Instant::now() + Duration::from_millis(10);

    let outcome = race_engine_guards(&mut engine, None, Some(deadline)).await;
    assert!(matches!(outcome, GuardOutcome::TimedOut));

    // The engine future was borrowed, not consumed/dropped by the race — the
    // caller can still poll (drain) it. It is still pending here.
    let drained = tokio::time::timeout(Duration::from_millis(10), &mut engine).await;
    assert!(
        drained.is_err(),
        "engine future should still be pending (not dropped by the race)"
    );
}

#[tokio::test]
async fn canceled_when_token_fires() {
    let token = CancellationToken::new();
    token.cancel();
    let mut engine = Box::pin(std::future::pending::<i32>());
    assert!(matches!(
        race_engine_guards(&mut engine, Some(&token), None).await,
        GuardOutcome::Canceled
    ));
}

#[test]
fn timeout_error_message_names_the_limit() {
    let boxed = CrawlGuardError::Timeout(7200).into_boxed();
    let msg = boxed.to_string();
    assert!(msg.contains("7200s"), "got: {msg}");
    assert!(msg.contains("crawl_job_timeout_secs"), "got: {msg}");
}

use super::*;

#[tokio::test(start_paused = true)]
async fn indeterminate_ownership_read_does_not_abort_extraction() {
    let shutdown = CancellationToken::new();
    let extraction = async {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        42
    };
    let result = drive_extract_with_heartbeat(
        extraction,
        &shutdown,
        std::time::Duration::from_secs(1),
        || async { AttemptOwnership::Unknown },
    )
    .await
    .expect("an ownership read error must not abort extraction");
    assert_eq!(result, 42);
}

#[tokio::test(start_paused = true)]
async fn confirmed_ownership_loss_aborts_extraction() {
    let shutdown = CancellationToken::new();
    let error = drive_extract_with_heartbeat(
        std::future::pending::<()>(),
        &shutdown,
        std::time::Duration::from_secs(1),
        || async { AttemptOwnership::Lost },
    )
    .await
    .unwrap_err();
    assert!(error.message.contains("no longer active"));
}

#[tokio::test(start_paused = true)]
async fn stalled_ownership_read_does_not_hide_extraction_completion() {
    let shutdown = CancellationToken::new();
    let extraction = async {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        42
    };
    let result = drive_extract_with_heartbeat(
        extraction,
        &shutdown,
        std::time::Duration::from_secs(1),
        std::future::pending::<AttemptOwnership>,
    )
    .await
    .expect("a stalled ownership read must not hide extraction completion");
    assert_eq!(result, 42);
}

#[tokio::test(start_paused = true)]
async fn stalled_ownership_read_does_not_hide_shutdown() {
    let shutdown = CancellationToken::new();
    let cancel = shutdown.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        cancel.cancel();
    });
    let error = drive_extract_with_heartbeat(
        std::future::pending::<()>(),
        &shutdown,
        std::time::Duration::from_secs(1),
        std::future::pending::<AttemptOwnership>,
    )
    .await
    .unwrap_err();
    assert!(error.message.contains("canceled"));
}

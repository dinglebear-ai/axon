use super::*;

// ── LogLevel::from(&str) ──────────────────────────────────────────────────

#[test]
fn log_level_from_str_warn_variants() {
    assert_eq!(LogLevel::from("warn"), LogLevel::Warn);
    assert_eq!(LogLevel::from("warning"), LogLevel::Warn);
}

#[test]
fn log_level_from_str_error() {
    assert_eq!(LogLevel::from("error"), LogLevel::Error);
}

#[test]
fn log_level_from_str_info() {
    assert_eq!(LogLevel::from("info"), LogLevel::Info);
}

/// Anything not explicitly matched ("debug", "trace") falls through to the
/// `_` arm which returns `Info` — the default variant.
/// Matching is case-insensitive, so uppercase variants are now handled.
#[test]
fn log_level_from_str_unknown_defaults_to_info() {
    assert_eq!(LogLevel::from("debug"), LogLevel::Info);
    assert_eq!(LogLevel::from("trace"), LogLevel::Info);
    // Case-insensitive: "WARN" now maps to Warn, not Info.
    assert_eq!(LogLevel::from("WARN"), LogLevel::Warn);
    assert_eq!(LogLevel::from("ERROR"), LogLevel::Error);
    assert_eq!(LogLevel::from("WARNING"), LogLevel::Warn);
}

#[test]
fn log_level_from_str_empty_defaults() {
    assert_eq!(LogLevel::from(""), LogLevel::Info);
}

// ── Display round-trips ───────────────────────────────────────────────────

#[test]
fn log_level_display_round_trips() {
    assert_eq!(format!("{}", LogLevel::Info), "info");
    assert_eq!(format!("{}", LogLevel::Warn), "warn");
    assert_eq!(format!("{}", LogLevel::Error), "error");
}

// ── emit() ────────────────────────────────────────────────────────────────

fn make_log_event(msg: &str) -> ServiceEvent {
    ServiceEvent::Log {
        level: LogLevel::Info,
        message: msg.to_string(),
    }
}

#[tokio::test]
async fn emit_delivers_event_when_channel_has_room() {
    let (tx, mut rx) = mpsc::channel::<ServiceEvent>(4);
    let sender_opt = Some(tx);
    emit(&sender_opt, make_log_event("hello")).await;
    let received = rx.try_recv().expect("event should have been delivered");
    assert_eq!(received, make_log_event("hello"));
}

#[tokio::test]
async fn emit_with_none_sender_does_not_panic() {
    // Passing None must not panic; the event is silently discarded.
    emit(&None, make_log_event("ignored")).await;
}

#[tokio::test]
async fn emit_blocks_on_full_channel_and_delivers_after_drain() {
    let (tx, mut rx) = mpsc::channel::<ServiceEvent>(1);
    let sender_opt = Some(tx);
    // Fill the channel
    emit(&sender_opt, make_log_event("first")).await;
    // Spawn a task to send to a full channel — will block until receiver drains
    let sender_clone = sender_opt.clone();
    let send_task = tokio::spawn(async move {
        emit(&sender_clone, make_log_event("second")).await;
    });
    // Drain the channel
    let _ = rx.recv().await;
    // Now the send_task should complete
    send_task.await.unwrap();
    // Second event must be delivered (not dropped)
    let msg = rx.recv().await;
    assert!(
        msg.is_some(),
        "second event must be delivered after drain, not dropped"
    );
}

use super::*;

#[test]
fn history_query_forwards_cursor_limit_and_status() {
    let request = watch_history_request(
        WatchId::new("watch_123"),
        WatchHistoryQuery {
            limit: Some(17),
            cursor: Some("cursor_abc".to_string()),
            status: Some(LifecycleStatus::Failed),
        },
    );

    assert_eq!(request.watch_id.0, "watch_123");
    assert_eq!(request.limit, Some(17));
    assert_eq!(request.cursor.as_deref(), Some("cursor_abc"));
    assert_eq!(request.status, Some(LifecycleStatus::Failed));
}

#[test]
fn history_query_preserves_absent_filters() {
    let request = watch_history_request(WatchId::new("watch_123"), WatchHistoryQuery::default());

    assert_eq!(request.limit, None);
    assert_eq!(request.cursor, None);
    assert_eq!(request.status, None);
}

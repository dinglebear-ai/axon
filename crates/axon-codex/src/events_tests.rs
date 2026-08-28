use super::*;
use serde_json::json;

#[test]
fn records_monotonic_events_and_rejects_cross_boot_cursors() {
    let recorder = EventRecorder::new(RuntimeEpoch(9));
    let first = recorder.record(EventKind::Exited);
    let second = recorder.record(EventKind::Exited);
    assert_eq!(first.cursor.sequence, 1);
    assert_eq!(second.cursor.sequence, 2);
    assert_eq!(recorder.after(Some(first.cursor), 10).unwrap(), [second]);
    assert!(
        recorder
            .after(
                Some(EventCursor {
                    boot_id: 8,
                    sequence: 1
                }),
                10
            )
            .unwrap_err()
            .contains("previous runtime")
    );
}

#[test]
fn redacts_secret_fields_and_bounds_strings() {
    let recorder = EventRecorder::new(RuntimeEpoch(1));
    let event = recorder.record(EventKind::Notification {
        method: "warning".to_string(),
        params: json!({"accessToken":"abc", "message":"x".repeat(5000)}),
    });
    let encoded = serde_json::to_string(&event).unwrap();
    assert!(!encoded.contains("abc"));
    assert!(encoded.contains("[REDACTED]"));
    assert!(encoded.len() < 4500);
}

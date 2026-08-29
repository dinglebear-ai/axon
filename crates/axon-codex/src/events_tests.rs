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

#[test]
fn wide_event_payloads_are_bounded_by_encoded_size() {
    let recorder = EventRecorder::new(RuntimeEpoch(1));
    let wide = (0..100)
        .map(|index| (format!("field_{index}"), json!("x".repeat(4096))))
        .collect::<serde_json::Map<_, _>>();
    let event = recorder.record(EventKind::Notification {
        method: "wide".to_string(),
        params: Value::Object(wide),
    });
    let encoded = serde_json::to_vec(&event).unwrap();
    assert!(encoded.len() < MAX_EVENT_PAYLOAD_BYTES + 1024);
    assert!(String::from_utf8(encoded).unwrap().contains("truncated"));
}

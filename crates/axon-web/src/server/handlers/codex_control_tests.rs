use super::*;

#[test]
fn read_body_rejects_mutation_actions() {
    assert!(
        serde_json::from_value::<CodexReadBody>(serde_json::json!({
            "action": "plugin_install"
        }))
        .is_err()
    );
}

#[test]
fn event_cursor_requires_both_components() {
    assert!(event_cursor(None, None).unwrap().is_none());
    assert!(event_cursor(Some(1), Some(2)).unwrap().is_some());
    assert!(event_cursor(Some(1), None).is_err());
    assert!(event_cursor(None, Some(2)).is_err());
}

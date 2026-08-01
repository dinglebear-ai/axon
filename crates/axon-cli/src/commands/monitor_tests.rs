use super::*;

#[test]
fn json_monitor_emits_an_empty_array_when_there_are_no_events() {
    let mut output = Vec::new();

    emit_events_to(&mut output, &[], false, true).unwrap();

    assert_eq!(output, b"[]\n");
}

#[test]
fn watched_json_uses_a_valid_jsonl_stream() {
    assert!(effective_jsonl(true, false, true));
    assert!(!effective_jsonl(false, false, true));
    assert!(effective_jsonl(false, true, false));
}

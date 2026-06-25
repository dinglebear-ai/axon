use super::{
    MAX_SSE_LINE_BYTES, done_answer_from_value, drain_sse_lines,
    parse_sse_data_line as parse_data_line, sentence_label,
};

#[test]
fn parse_sse_data_line() {
    assert_eq!(
        parse_data_line("data: {\"type\":\"delta\",\"text\":\"hi\"}"),
        Some("{\"type\":\"delta\",\"text\":\"hi\"}".to_string())
    );
}

#[test]
fn ignores_non_data_sse_line() {
    assert_eq!(parse_data_line("event: delta"), None);
}

#[test]
fn buffers_split_utf8_until_complete_line() {
    let mut pending = Vec::new();
    let snowman = "data: {\"type\":\"delta\",\"text\":\"☃\"}\n".as_bytes();
    assert!(
        drain_sse_lines(&mut pending, &snowman[..snowman.len() - 2])
            .unwrap()
            .is_empty()
    );

    let lines = drain_sse_lines(&mut pending, &snowman[snowman.len() - 2..]).unwrap();

    assert_eq!(lines, vec!["data: {\"type\":\"delta\",\"text\":\"☃\"}"]);
    assert!(pending.is_empty());
}

#[test]
fn rejects_oversized_sse_line() {
    let mut pending = Vec::new();
    // Build a chunk larger than MAX_SSE_LINE_BYTES with no newline.
    let big_chunk = vec![b'x'; MAX_SSE_LINE_BYTES + 1];
    let result = drain_sse_lines(&mut pending, &big_chunk);
    assert!(
        result.is_err(),
        "oversized lineless chunk must return an error"
    );
    let msg = result.unwrap_err();
    assert!(
        msg.contains("exceeded") || msg.contains("exceeds"),
        "error message should describe the size violation: {msg}"
    );
}

#[test]
fn accepts_valid_sse_event_within_limit() {
    let mut pending = Vec::new();
    let line = "data: {\"type\":\"done\",\"answer\":\"ok\"}\n";
    let lines = drain_sse_lines(&mut pending, line.as_bytes()).unwrap();
    assert_eq!(lines, vec!["data: {\"type\":\"done\",\"answer\":\"ok\"}"]);
    assert!(pending.is_empty());
}

#[test]
fn rejects_oversized_sse_line_with_trailing_newline() {
    let mut pending = Vec::new();
    // Build a line that is longer than MAX_SSE_LINE_BYTES and IS followed by a
    // newline — exercises the second size check inside the newline-scanning loop.
    let mut big_line = vec![b'x'; MAX_SSE_LINE_BYTES + 1];
    big_line.push(b'\n');
    let result = drain_sse_lines(&mut pending, &big_line);
    assert!(
        result.is_err(),
        "oversized line with newline must return an error"
    );
    let msg = result.unwrap_err();
    assert!(
        msg.contains("exceeded") || msg.contains("exceeds"),
        "error message should describe the size violation: {msg}"
    );
}

#[test]
fn crlf_sse_lines_are_decoded_correctly() {
    let mut pending = Vec::new();
    let line = "data: {\"type\":\"done\",\"answer\":\"ok\"}\r\n";
    let lines = drain_sse_lines(&mut pending, line.as_bytes()).unwrap();
    assert_eq!(lines, vec!["data: {\"type\":\"done\",\"answer\":\"ok\"}"]);
    assert!(pending.is_empty());
}

#[test]
fn parse_sse_data_line_handles_no_space_after_colon() {
    // strip_prefix("data:") then .trim() — so "data:{...}" is valid
    assert_eq!(
        parse_data_line("data:{\"type\":\"delta\"}"),
        Some("{\"type\":\"delta\"}".to_string())
    );
}

#[test]
fn done_answer_reads_stream_result_payload() {
    let value = serde_json::json!({
        "type": "done",
        "result": {
            "answer": "normalized answer\n\n## Sources\n- [S1] https://docs.example.com"
        }
    });

    assert_eq!(
        done_answer_from_value(&value).as_deref(),
        Some("normalized answer\n\n## Sources\n- [S1] https://docs.example.com")
    );
}

#[test]
fn sentence_label_formats_stream_phase() {
    assert_eq!(sentence_label("retrieving"), "Retrieving");
    assert_eq!(sentence_label("context_build"), "Context build");
}

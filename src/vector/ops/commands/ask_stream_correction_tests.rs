use super::normalized_stream_correction_text;

#[test]
fn labels_stored_normalized_answer() {
    let rendered = normalized_stream_correction_text(
        "Answer with normalized citations [S1].\n\n## Sources\n- [S1] https://docs.example.com",
    );

    assert!(rendered.contains("Normalized answer (stored for JSON and follow-up sessions):"));
    assert!(rendered.contains("Answer with normalized citations [S1]."));
    assert!(rendered.starts_with("\n\n---\n\n"));
}

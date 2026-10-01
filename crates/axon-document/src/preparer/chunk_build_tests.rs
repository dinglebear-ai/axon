use super::*;

#[test]
fn embedding_backstop_keeps_short_code_lines_intact() {
    let mut source = (0..25)
        .map(|_| format!("{}\n", "x".repeat(75)))
        .collect::<String>();
    source.push_str(&format!("{}\n", "x".repeat(54)));
    source.push_str("let credential =\n    validate_transport_credential_binding(foo);\n");
    assert_eq!(
        axon_core::redact::retrievable_body_secret_detector(&source),
        None
    );
    let chunk = DocumentChunk::new(source.clone(), text::source_range(&source, 0, source.len()));
    let (parts, split) = bound_embedding_chunks(vec![chunk], &source);
    assert!(split);
    assert_eq!(
        parts
            .iter()
            .map(|part| part.content.as_str())
            .collect::<String>(),
        source
    );
    assert!(parts.iter().all(|part| {
        axon_core::redact::retrievable_body_secret_detector(&part.content).is_none()
    }));
    assert!(
        parts
            .iter()
            .all(|part| part.content.len() <= text::MAX_PLAIN_TEXT_CHUNK_BYTES)
    );
}

#[test]
fn no_raw_empty_fallback_reintroduces_punctuation_junk() {
    let source = "{}();".repeat(30);
    let bounded = bound_or_fallback(Vec::new(), &source);
    assert!(bounded.empty_fallback);
    assert!(crate::quality::useful_chunks(bounded.chunks, &source).is_empty());
}

#[test]
fn scaffold_omission_precedes_dedup_of_identical_fenced_body_text() {
    let heading = "# Useful heading and matching fenced comment with enough meaningful context";
    let source = format!("{heading}\n\n```bash\n{heading}\n```\n");
    let body_start = source.rfind(heading).unwrap();
    let chunks = [0, body_start]
        .into_iter()
        .map(|start| {
            DocumentChunk::new(
                heading,
                text::source_range(&source, start, start + heading.len()),
            )
            .with_title("Shared breadcrumb")
            .with_heading_path(vec!["Shared breadcrumb".into()])
        })
        .collect();
    let output = finalize_chunks(
        ChunkingProfile::MarkdownSections,
        chunks,
        &source,
        MarkdownChunkLimits::new(96, 1, 0),
        "heading_sections",
        50,
    );
    assert_eq!(output.len(), 1, "lost the identical fenced body comment");
    assert_eq!(output[0].content, heading);
    assert_eq!(output[0].range.byte_start, Some(body_start as u64));
    assert!(!output[0].metadata.contains_key("additional_source_ranges"));
}

use super::*;

#[test]
fn yaml_records_splits_top_level_keys() {
    let text = "name: axon\nversion: 1.0\nnested:\n  a: 1\n  b: 2\n";
    let chunks = yaml_records(text);
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].range.yaml_path.as_deref(), Some("name"));
    assert_eq!(chunks[2].range.yaml_path.as_deref(), Some("nested"));
    assert!(chunks[2].content.contains("a: 1"));
}

#[test]
fn toml_records_splits_top_level_tables() {
    let text = "[package]\nname = \"axon\"\n\n[dependencies]\nserde = \"1\"\n";
    let chunks = toml_records(text);
    assert_eq!(chunks.len(), 2);
    assert!(chunks[0].content.contains("name = \"axon\""));
    assert!(chunks[1].content.contains("serde"));
}

#[test]
fn csv_records_produce_one_chunk_per_data_row_with_real_row_index() {
    let text = "name,age\nalice,30\nbob,40\n";
    let chunks = csv_records(text);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].content, "name: alice\nage: 30");
    assert_eq!(chunks[0].range.csv_row, Some(0));
    assert_eq!(chunks[1].range.csv_row, Some(1));
}

#[test]
fn xml_records_splits_top_level_children() {
    let text = "<root><a>1</a><b>2</b></root>";
    let chunks = xml_records(text);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].content, "<a>1</a>");
    assert_eq!(chunks[0].range.xml_xpath.as_deref(), Some("/*/a"));
    assert_eq!(chunks[1].content, "<b>2</b>");
}

#[test]
fn xml_records_returns_empty_for_malformed_input() {
    let chunks = xml_records("not xml at all");
    assert!(chunks.is_empty());
}

#[test]
fn structured_record_ranges_match_legacy_offsets_without_prefix_rescans() {
    let cases: Vec<(String, fn(&str) -> Vec<DocumentChunk>)> = vec![
        (
            format!("name,value\r\n{}", "café,東京\r\n".repeat(512)),
            csv_records,
        ),
        ("clé: 東京\r\n".repeat(512), yaml_records),
        ("[café]\r\nvalue = \"東京\"\r\n".repeat(512), toml_records),
        (
            format!("<root>{}</root>", "<item>東京</item>\r\n".repeat(512)),
            xml_records,
        ),
    ];
    for (source, chunker) in cases {
        let (chunks, work) = crate::performance_measurement::measure(|| chunker(&source));
        assert_eq!(chunks.len(), 512);
        assert!(work.range_scan_bytes <= source.len() * 4);
        for chunk in chunks {
            let expected = crate::text::source_range(
                &source,
                chunk.range.byte_start.unwrap() as usize,
                chunk.range.byte_end.unwrap() as usize,
            );
            assert_eq!(chunk.range.char_start, expected.char_start);
            assert_eq!(chunk.range.char_end, expected.char_end);
            assert_eq!(chunk.range.line_start, expected.line_start);
            assert_eq!(chunk.range.line_end, expected.line_end);
        }
    }
}

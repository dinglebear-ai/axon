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

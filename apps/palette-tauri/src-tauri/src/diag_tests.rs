use super::*;

#[test]
fn append_to_writes_a_timestamped_line() {
    let dir = std::env::temp_dir().join(format!("axon-diag-{}", uuid::Uuid::new_v4()));
    let path = dir.join("palette.log");
    append_to(&path, "hello world").unwrap();
    let contents = std::fs::read_to_string(&path).unwrap();
    assert!(
        contents.contains("WARN palette: hello world"),
        "got: {contents}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn append_to_is_additive_across_calls() {
    let dir = std::env::temp_dir().join(format!("axon-diag-{}", uuid::Uuid::new_v4()));
    let path = dir.join("palette.log");
    append_to(&path, "first").unwrap();
    append_to(&path, "second").unwrap();
    let contents = std::fs::read_to_string(&path).unwrap();
    assert!(contents.contains("first") && contents.contains("second"));
    assert_eq!(contents.lines().count(), 2);
    std::fs::remove_dir_all(&dir).ok();
}

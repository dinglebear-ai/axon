use super::*;

fn entry(name: &str) -> SftpEntry {
    SftpEntry {
        name: name.to_string(),
        path: name.to_string(),
        is_dir: false,
        size: 0,
        modified_unix: None,
    }
}

#[test]
fn directory_packet_admission_stops_at_the_exact_limit() {
    let mut entries = Vec::new();
    assert!(!push_entry_bounded(&mut entries, entry("a"), 2));
    assert!(!push_entry_bounded(&mut entries, entry("b"), 2));
    assert!(push_entry_bounded(&mut entries, entry("c"), 2));
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "a");
    assert_eq!(entries[1].name, "b");
}

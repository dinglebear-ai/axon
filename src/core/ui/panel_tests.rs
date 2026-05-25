use super::*;

#[test]
fn panel_renders_box_drawing_borders() {
    let s = panel_plain(
        "Crawl complete",
        &[("pages", "42"), ("chunks", "1024"), ("elapsed", "12.3s")],
    );
    assert!(s.contains("╭"));
    assert!(s.contains("╮"));
    assert!(s.contains("╰"));
    assert!(s.contains("╯"));
    assert!(s.contains("Crawl complete"));
    assert!(s.contains("pages"));
    assert!(s.contains("42"));
}

#[test]
fn panel_handles_empty_rows() {
    let s = panel_plain("Done", &[]);
    assert!(s.contains("Done"));
    assert!(s.lines().count() >= 2);
}

#[test]
fn panel_aligns_widest_key() {
    let s = panel_plain("X", &[("short", "1"), ("a much longer key", "2")]);
    // Each row starts with "│ <key padded to widest>  <value>..."; the value
    // column should start at the same byte offset on every row.
    let rows: Vec<&str> = s.lines().filter(|l| l.starts_with('│')).collect();
    assert_eq!(rows.len(), 2);
    let positions: Vec<_> = rows
        .iter()
        .map(|l| l.find(|c: char| c.is_ascii_digit()).unwrap_or(0))
        .collect();
    assert!(positions.windows(2).all(|w| w[0] == w[1]));
}

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
    let rows: Vec<&str> = s.lines().filter(|l| l.starts_with('│')).collect();
    assert_eq!(rows.len(), 2);
    let positions: Vec<_> = rows
        .iter()
        .map(|l| l.find(|c: char| c.is_ascii_digit()).unwrap_or(0))
        .collect();
    assert!(positions.windows(2).all(|w| w[0] == w[1]));
}

#[test]
fn panel_every_line_has_identical_visible_width() {
    let s = panel_plain(
        "Crawl complete",
        &[("pages", "42"), ("chunks", "1024"), ("elapsed", "12.3s")],
    );
    let widths: Vec<usize> = s.lines().map(|l| l.chars().count()).collect();
    assert!(!widths.is_empty());
    let first = widths[0];
    assert!(
        widths.iter().all(|&w| w == first),
        "all panel lines must have identical char count, got: {widths:?}"
    );
}

#[test]
fn panel_right_border_aligns_when_title_is_wider_than_body() {
    // Title (25 chars) is much wider than body ("a 1" = 3 visible chars). Pre-fix,
    // the row's right `│` sat one column past the top `╮`. Assert exact column.
    let s = panel_plain("This is a very long title", &[("a", "1"), ("b", "2")]);
    let line_widths: Vec<usize> = s.lines().map(|l| l.chars().count()).collect();
    let first = line_widths[0];
    assert!(
        line_widths.iter().all(|&w| w == first),
        "all lines must align when title is wider than body, got: {line_widths:?}"
    );
}

#[test]
fn panel_aligns_with_multibyte_key() {
    // Sparkline-style emoji / multibyte keys must use chars().count(), not len().
    // Regression guard: if padding ever switches to byte length, both rows
    // drift apart even though both keys are 3 chars.
    let s = panel_plain("Stats", &[("café", "1"), ("axe", "2"), ("longer-key", "3")]);
    let widths: Vec<usize> = s.lines().map(|l| l.chars().count()).collect();
    let first = widths[0];
    assert!(
        widths.iter().all(|&w| w == first),
        "multibyte keys must align, got: {widths:?}"
    );
}

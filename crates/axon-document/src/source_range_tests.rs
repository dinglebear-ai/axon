use axon_api::source::SourceRange;

use super::{SourceRangeBounds, validate_source_range};

fn bounds() -> SourceRangeBounds {
    SourceRangeBounds {
        line_count: 3,
        byte_len: 12,
        char_count: 12,
    }
}

fn empty_range() -> SourceRange {
    SourceRange {
        line_start: None,
        line_end: None,
        byte_start: None,
        byte_end: None,
        char_start: None,
        char_end: None,
        time_start_ms: None,
        time_end_ms: None,
        dom_selector: None,
        json_pointer: None,
        yaml_path: None,
        xml_xpath: None,
        csv_row: None,
        session_turn_id: None,
        turn_start: None,
        turn_end: None,
    }
}

#[test]
fn one_sided_positions_inside_bounds_are_valid() {
    let mut range = empty_range();
    range.line_start = Some(3);
    range.byte_start = Some(12);
    range.char_start = Some(12);

    validate_source_range(&range, &bounds()).expect("one-sided in-bounds range is valid");
}

#[test]
fn one_sided_line_outside_bounds_is_rejected() {
    let mut range = empty_range();
    range.line_start = Some(4);

    let err = validate_source_range(&range, &bounds()).expect_err("line_start past document");
    assert!(err.contains("line_start exceeds bounds"), "{err}");
}

#[test]
fn one_sided_byte_outside_bounds_is_rejected() {
    let mut range = empty_range();
    range.byte_end = Some(13);

    let err = validate_source_range(&range, &bounds()).expect_err("byte_end past document");
    assert!(err.contains("byte_end exceeds bounds"), "{err}");
}

#[test]
fn one_sided_char_outside_bounds_is_rejected() {
    let mut range = empty_range();
    range.char_start = Some(13);

    let err = validate_source_range(&range, &bounds()).expect_err("char_start past document");
    assert!(err.contains("char_start exceeds bounds"), "{err}");
}

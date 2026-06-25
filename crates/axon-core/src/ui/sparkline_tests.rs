use super::*;

#[test]
fn sparkline_handles_empty() {
    assert_eq!(sparkline_plain(&[]), "");
}

#[test]
fn sparkline_handles_single_value() {
    let s = sparkline_plain(&[5]);
    assert_eq!(s.chars().count(), 1);
}

#[test]
fn sparkline_maps_min_to_lowest_block() {
    let s = sparkline_plain(&[0, 1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(s.chars().count(), 8);
    assert!(s.starts_with('▁'));
    assert!(s.ends_with('█'));
}

#[test]
fn sparkline_all_equal_values_renders_mid_block() {
    let s = sparkline_plain(&[5, 5, 5, 5]);
    assert_eq!(s.chars().count(), 4);
    let first = s.chars().next().unwrap();
    assert!(s.chars().all(|c| c == first));
}

use super::*;

#[test]
fn fresh_collection_starts_at_zero() {
    let unique = format!("test_gen_fresh_{}", uuid::Uuid::new_v4());
    assert_eq!(current_generation(&unique), 0);
}

#[test]
fn bump_increments_monotonically() {
    let unique = format!("test_gen_bump_{}", uuid::Uuid::new_v4());
    assert_eq!(current_generation(&unique), 0);
    let g1 = bump_generation(&unique);
    let g2 = bump_generation(&unique);
    assert_eq!(g1, 1);
    assert_eq!(g2, 2);
    assert_eq!(current_generation(&unique), 2);
}

#[test]
fn collections_are_independent() {
    let a = format!("test_gen_iso_a_{}", uuid::Uuid::new_v4());
    let b = format!("test_gen_iso_b_{}", uuid::Uuid::new_v4());
    bump_generation(&a);
    bump_generation(&a);
    assert_eq!(current_generation(&a), 2);
    assert_eq!(current_generation(&b), 0);
}

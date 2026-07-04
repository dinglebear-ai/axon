use super::*;
use axon_api::source::AuthorityLevel;

#[test]
fn ranking_is_total_and_ordered() {
    assert!(Authority::UserPinned.rank() > Authority::Official.rank());
    assert!(Authority::Official.rank() > Authority::Verified.rank());
    assert!(Authority::Verified.rank() > Authority::Inferred.rank());
    assert!(Authority::Inferred.rank() > Authority::Mirror.rank());
    assert!(Authority::Mirror.rank() > Authority::Community.rank());
    assert!(Authority::Community.rank() > Authority::Unknown.rank());
}

#[test]
fn only_verified_and_above_are_authoritative() {
    assert!(Authority::UserPinned.is_authoritative());
    assert!(Authority::Official.is_authoritative());
    assert!(Authority::Verified.is_authoritative());
    assert!(!Authority::Inferred.is_authoritative());
    assert!(!Authority::Community.is_authoritative());
    assert!(!Authority::Unknown.is_authoritative());
}

#[test]
fn level_roundtrip_conflicting_collapses_to_unknown() {
    for level in [
        AuthorityLevel::Official,
        AuthorityLevel::Verified,
        AuthorityLevel::UserPinned,
        AuthorityLevel::Inferred,
        AuthorityLevel::Community,
        AuthorityLevel::Mirror,
        AuthorityLevel::Unknown,
    ] {
        let a = Authority::from_level(level);
        assert_eq!(a.to_level(), level, "roundtrip failed for {level:?}");
    }
    // Conflicting is not an input authority; it collapses to Unknown.
    assert_eq!(
        Authority::from_level(AuthorityLevel::Conflicting),
        Authority::Unknown
    );
}

#[test]
fn higher_authority_wins_without_conflict() {
    let d = resolve_authority(Authority::Inferred, Authority::Official);
    assert_eq!(d.winner, Authority::Official);
    assert!(!d.conflict);

    let d = resolve_authority(Authority::Official, Authority::Inferred);
    assert_eq!(d.winner, Authority::Official);
    assert!(!d.conflict);
}

#[test]
fn equal_authoritative_claims_conflict_but_inferred_ties_do_not() {
    // Two official claims of equal rank disagree → conflict, keep existing.
    let d = resolve_authority(Authority::Official, Authority::Official);
    assert_eq!(d.winner, Authority::Official);
    assert!(d.conflict);

    // Two inferred (non-authoritative) claims tie without conflict.
    let d = resolve_authority(Authority::Inferred, Authority::Inferred);
    assert_eq!(d.winner, Authority::Inferred);
    assert!(!d.conflict);
}

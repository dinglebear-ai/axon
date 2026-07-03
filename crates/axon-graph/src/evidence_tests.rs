use super::*;
use crate::authority::Authority;

#[test]
fn all_evidence_kinds_roundtrip() {
    for kind in EvidenceKind::ALL {
        let parsed = EvidenceKind::from_str(kind.as_str()).unwrap();
        assert_eq!(*kind, parsed);
    }
}

#[test]
fn evidence_kind_count_matches_registry() {
    // 32 evidence kinds are defined in source-graph.md "Evidence Kinds".
    assert_eq!(EvidenceKind::ALL.len(), 32);
}

#[test]
fn unknown_evidence_kind_is_rejected_by_from_str() {
    assert!(EvidenceKind::from_str("made_up_kind").is_err());
}

#[test]
fn authority_mapping_follows_conflict_rules() {
    // User-pinned mappings win.
    assert_eq!(EvidenceKind::UserPinned.authority(), Authority::UserPinned);
    // Official package/repo metadata outranks community/derived.
    assert_eq!(
        EvidenceKind::PackageRepository.authority(),
        Authority::Official
    );
    assert_eq!(
        EvidenceKind::GithubHomepage.authority(),
        Authority::Official
    );
    // Derived-source attribution is a mirror, not official.
    assert_eq!(
        EvidenceKind::DerivedSourceAttribution.authority(),
        Authority::Mirror
    );
    // Low-confidence text mentions are inferred, never authoritative.
    assert_eq!(EvidenceKind::TextMention.authority(), Authority::Inferred);
    assert!(!EvidenceKind::TextMention.authority().is_authoritative());
}

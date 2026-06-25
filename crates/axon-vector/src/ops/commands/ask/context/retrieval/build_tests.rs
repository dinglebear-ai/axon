use super::*;
use crate::ops::commands::retrieval::CandidateBuildPolicy;

#[test]
fn keyword_search_failure_returns_warning() {
    let secondary: SearchHitsResult = Err("keyword backend timed out".into());
    let built = build_ask_candidates(
        Vec::new(),
        Some(secondary),
        &CandidateBuildPolicy {
            allow_low_signal: false,
            allow_short_content: false,
        },
        None,
    );

    assert_eq!(built.warnings.len(), 1);
    assert!(
        built.warnings[0].contains("keyword search failed"),
        "warning should describe degraded keyword retrieval: {:?}",
        built.warnings
    );
    assert!(!built.warnings[0].contains("keyword backend timed out"));
}

use super::diversify_documents;

#[test]
fn complementary_spans_survive_without_crowding_other_documents() {
    let mut hits = vec![("a", 1), ("a", 2), ("a", 3), ("b", 4), ("a", 5), ("c", 6)];
    diversify_documents(&mut hits, |hit| hit.0);
    assert_eq!(
        hits,
        [("a", 1), ("a", 2), ("b", 4), ("c", 6), ("a", 3), ("a", 5)]
    );
    // This is an ordering policy; no evidence or score is dropped or rewritten.
    assert_eq!(hits.iter().map(|h| h.1).sum::<i32>(), 21);
}

#[test]
fn single_document_and_empty_results_keep_provider_order() {
    let mut hits = vec![1, 2, 3, 4];
    diversify_documents(&mut hits, |_| "only-document");
    assert_eq!(hits, [1, 2, 3, 4]);
    let mut empty: Vec<i32> = vec![];
    diversify_documents(&mut empty, |_| "none");
    assert!(empty.is_empty());
}

#[test]
fn document_identity_includes_source_scope_and_order_is_deterministic() {
    let input = vec![
        ("source-a", "doc", 1),
        ("source-a", "doc", 2),
        ("source-a", "doc", 3),
        ("source-b", "doc", 4),
    ];
    for _ in 0..8 {
        let mut hits = input.clone();
        diversify_documents(&mut hits, |h| (h.0, h.1));
        assert_eq!(hits.iter().map(|h| h.2).collect::<Vec<_>>(), [1, 2, 4, 3]);
    }
}

#[test]
fn diversity_preserves_every_page_evidence_set_and_partial_tail() {
    for length in 0..74 {
        let original = (0..length).map(|i| (i % 3, i)).collect::<Vec<_>>();
        let mut hits = original.clone();
        diversify_documents(&mut hits, |h| h.0);
        assert_eq!(hits.len(), original.len());
        for (before, after) in original.chunks(10).zip(hits.chunks(10)) {
            let mut before_ids = before.iter().map(|h| h.1).collect::<Vec<_>>();
            let mut after_ids = after.iter().map(|h| h.1).collect::<Vec<_>>();
            before_ids.sort_unstable();
            after_ids.sort_unstable();
            assert_eq!(before_ids, after_ids);
        }
    }
}

use super::*;

#[test]
fn carry_forward_filter_contains_only_the_requested_key_batch() {
    let keys = vec!["a".to_string(), "b".to_string()];
    let filter = carry_forward_filter(&SourceId::new("source"), 7, &keys);
    assert_eq!(filter["must"][0]["match"]["value"], "source");
    assert_eq!(filter["must"][1]["match"]["value"], 7);
    assert_eq!(
        filter["must"][2]["match"]["any"],
        serde_json::json!(["a", "b"])
    );
}

#[test]
fn carry_forward_key_batches_bound_match_any_requests() {
    let keys = (0..257)
        .map(|index| format!("key-{index}"))
        .collect::<Vec<_>>();
    let batches = keys
        .chunks(CARRY_FORWARD_KEY_BATCH_SIZE)
        .collect::<Vec<_>>();
    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0].len(), 256);
    assert_eq!(batches[1], ["key-256"]);
}

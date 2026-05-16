use super::parse_vote;

#[test]
fn parse_vote_accepts_rank() {
    assert_eq!(parse_vote("2\n", 3).ok(), Some(Some(2)));
}

#[test]
fn parse_vote_accepts_skip() {
    assert_eq!(parse_vote("skip", 3).ok(), Some(None));
    assert_eq!(parse_vote("s", 3).ok(), Some(None));
}

#[test]
fn parse_vote_rejects_out_of_range_rank() {
    assert!(parse_vote("4", 3).is_err());
    assert!(parse_vote("0", 3).is_err());
}

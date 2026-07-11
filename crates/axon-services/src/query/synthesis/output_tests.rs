use super::*;

#[test]
fn append_ask_delta_appends_deltas() {
    let mut answer = String::new();
    append_ask_delta(&mut answer, "Hello");
    append_ask_delta(&mut answer, " world");
    assert_eq!(answer, "Hello world");
}

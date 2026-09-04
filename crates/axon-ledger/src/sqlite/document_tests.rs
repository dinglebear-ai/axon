use super::*;

#[test]
fn one_status_api_call_uses_one_transaction() {
    assert_eq!(document_status_transaction_count(250), 1);
}

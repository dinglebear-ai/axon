use super::{validate_first_run_query, validate_first_run_url};

#[test]
fn first_run_url_rejects_empty_values() {
    assert_eq!(validate_first_run_url("").unwrap_err(), "url is required");
    assert_eq!(
        validate_first_run_url("  \t").unwrap_err(),
        "url is required"
    );
}

#[test]
fn first_run_url_trims_non_empty_values() {
    assert_eq!(
        validate_first_run_url(" https://example.com/docs ").unwrap(),
        "https://example.com/docs"
    );
}

#[test]
fn first_run_url_rejects_non_http_urls() {
    assert_eq!(
        validate_first_run_url("file:///etc/passwd").unwrap_err(),
        "url must be an http or https URL"
    );
    assert_eq!(
        validate_first_run_url("not a url").unwrap_err(),
        "url must be an http or https URL"
    );
}

#[test]
fn first_run_query_rejects_empty_values() {
    assert_eq!(
        validate_first_run_query("").unwrap_err(),
        "query is required"
    );
    assert_eq!(
        validate_first_run_query("  \n").unwrap_err(),
        "query is required"
    );
}

#[test]
fn first_run_query_trims_non_empty_values() {
    assert_eq!(
        validate_first_run_query(" what changed? ").unwrap(),
        "what changed?"
    );
}

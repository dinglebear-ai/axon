use super::*;

#[test]
fn forbidden_field_name_matches_known_fragments() {
    assert!(forbidden_field_name("Authorization"));
    assert!(forbidden_field_name("raw_auth_header"));
    assert!(!forbidden_field_name("chunk_text"));
}

#[test]
fn secret_like_field_name_matches_tokens() {
    assert!(secret_like_field_name("access_token"));
    assert!(secret_like_field_name("my_custom_token"));
    assert!(!secret_like_field_name("chunk_id"));
}

#[test]
fn value_contains_secret_matches_bearer_and_bare_tokens() {
    assert!(value_contains_secret("Authorization: Bearer abc123"));
    assert!(value_contains_secret("sk-proj-abcdefghijklmnopqrstuvwx"));
    assert!(!value_contains_secret("just some plain text"));
}

#[test]
fn value_is_absolute_local_path_matches_home_and_windows_paths() {
    assert!(value_is_absolute_local_path("/home/jacob/workspace"));
    assert!(value_is_absolute_local_path(r"C:\Users\jacob"));
    assert!(!value_is_absolute_local_path("https://example.com/home/"));
}

#[test]
fn raw_dotenv_assignment_matches_upper_snake_case_keys() {
    assert!(raw_dotenv_assignment("API_KEY=abc123"));
    assert!(!raw_dotenv_assignment("just a sentence = not env"));
}

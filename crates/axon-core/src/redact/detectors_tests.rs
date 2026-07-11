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

#[test]
fn contains_bare_secret_token_matches_all_github_token_prefixes() {
    for prefix in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
        let value = format!("{prefix}0123456789abcdefghij");
        assert!(
            contains_bare_secret_token(&value),
            "expected {prefix} to be detected"
        );
    }
    assert!(!contains_bare_secret_token(
        "gh_not_a_real_prefix_1234567890"
    ));
}

#[test]
fn contains_pem_private_key_block_matches_common_key_headers() {
    assert!(contains_pem_private_key_block(
        "-----BEGIN RSA PRIVATE KEY-----\nMIIBOgIBAAJ...\n-----END RSA PRIVATE KEY-----"
    ));
    assert!(contains_pem_private_key_block(
        "-----BEGIN PRIVATE KEY-----\nMIIBOgIBAAJ...\n-----END PRIVATE KEY-----"
    ));
    assert!(contains_pem_private_key_block(
        "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1r...\n-----END OPENSSH PRIVATE KEY-----"
    ));
    // Non-secret lookalikes: public keys and unrelated PEM-shaped headers.
    assert!(!contains_pem_private_key_block(
        "-----BEGIN PUBLIC KEY-----\nMIIBIjANBg...\n-----END PUBLIC KEY-----"
    ));
    assert!(!contains_pem_private_key_block(
        "-----BEGIN CERTIFICATE-----\nMIID...\n-----END CERTIFICATE-----"
    ));
    assert!(!contains_pem_private_key_block(
        "just some PRIVATE KEY text"
    ));
}

#[test]
fn contains_url_embedded_credentials_matches_user_and_password() {
    assert!(contains_url_embedded_credentials(
        "postgres://myuser:s3cr3tpass@db.internal:5432/mydb"
    ));
    assert!(contains_url_embedded_credentials(
        "https://admin:hunter2@example.com/path"
    ));
    // Non-secret lookalikes: bare username (no password), and a plain URL.
    assert!(!contains_url_embedded_credentials(
        "https://user@example.com/path"
    ));
    assert!(!contains_url_embedded_credentials(
        "https://example.com/a?b=c"
    ));
}

#[test]
fn looks_like_bare_cookie_string_matches_unlabeled_cookie_values() {
    assert!(looks_like_bare_cookie_string(
        "sessionid=9f8a7b6c5d4e3f2a1b0c; Path=/; HttpOnly"
    ));
    assert!(looks_like_bare_cookie_string(
        "csrftoken=abcdef0123456789abcdef01234567; SameSite=Lax"
    ));
    // Non-secret lookalikes: short trivial key=value pairs, and prose with
    // semicolons that isn't cookie-shaped at all.
    assert!(!looks_like_bare_cookie_string("a=1; b=2"));
    assert!(!looks_like_bare_cookie_string(
        "Alice went to the store; Bob stayed home"
    ));
    assert!(!looks_like_bare_cookie_string("just one segment"));
}

#[test]
fn field_is_opaque_token_context_matches_provider_and_generic_fragments() {
    assert!(field_is_opaque_token_context("gitlab_token"));
    assert!(field_is_opaque_token_context("gitea_deploy_token"));
    assert!(field_is_opaque_token_context("oauth_client_secret"));
    assert!(!field_is_opaque_token_context("web_title"));
}

#[test]
fn value_is_high_entropy_token_bounds_short_and_low_entropy_values() {
    assert!(value_is_high_entropy_token(
        "aK9fQ2mP7zT4xL8vN1cR6bY3wE0sJ5h"
    ));
    // Non-secret lookalikes: too short, or low-entropy repeated runs.
    assert!(!value_is_high_entropy_token("short"));
    assert!(!value_is_high_entropy_token(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    ));
}

#[test]
fn last_field_segment_splits_dotted_paths() {
    assert_eq!(last_field_segment("metadata.gitlab_token"), "gitlab_token");
    assert_eq!(last_field_segment("web_title"), "web_title");
}

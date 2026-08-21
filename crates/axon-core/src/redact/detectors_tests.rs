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
fn value_contains_secret_requires_credential_evidence() {
    assert!(!value_contains_secret("Authorization: Bearer abc123"));
    assert!(value_contains_secret(
        "Authorization: Bearer abcdef0123456789abcdef0123456789"
    ));
    assert!(value_contains_secret("sk-proj-abcdefghijklmnopqrstuvwx"));
    assert!(!value_contains_secret("just some plain text"));
}

#[test]
fn security_vocabulary_fields_are_not_secrets_without_credential_semantics() {
    for field in [
        "token_count",
        "token_estimate",
        "tokenizer",
        "password_policy",
        "secret_scanning_enabled",
        "credential_type",
        "cookie_policy",
        "authorization_status",
        "oauth_status",
        "gitlab_identifier",
        "page_token",
        "next_page_token",
        "continuation_token",
        "cursor_token",
        "tokenCount",
        "authorizationStatus",
        "pageToken",
    ] {
        assert!(
            !secret_like_field_name(field),
            "benign field was sensitive: {field}"
        );
        assert!(
            !forbidden_field_name(field),
            "benign field was forbidden: {field}"
        );
    }
}

#[test]
fn camel_case_credential_fields_are_classified() {
    for field in [
        "accessToken",
        "refreshToken",
        "clientSecret",
        "privateKey",
        "myApiKey",
    ] {
        assert!(
            secret_like_field_name(field),
            "missed camelCase secret field: {field}"
        );
    }
    for field in ["authorizationHeader", "rawAuthHeader", "cookieHeader"] {
        assert!(
            forbidden_field_name(field),
            "missed camelCase forbidden field: {field}"
        );
    }
}

#[test]
fn retrievable_body_detector_preserves_tutorial_examples_but_rejects_concrete_secrets() {
    for example in [
        "Use Authorization: Bearer secret-token in this request",
        "TOKEN=abc123",
        "passwd=hunter2",
        "postgres://user:password@localhost/app",
    ] {
        assert_eq!(
            retrievable_body_secret_detector(example),
            None,
            "tutorial example was treated as a concrete secret: {example}"
        );
    }

    let github = format!("ghp_{}", "a".repeat(24));
    assert_eq!(
        retrievable_body_secret_detector(&github),
        Some("bare_secret_token")
    );
    assert_eq!(
        retrievable_body_secret_detector("password=thisisarealpassphrase"),
        Some("secret_assignment")
    );
    assert_eq!(
        retrievable_body_secret_detector("postgres://admin:s3cr3tpass@db.internal/app"),
        Some("url_credentials")
    );
}

#[test]
fn every_known_token_family_is_classified_for_payload_guards() {
    for token in [
        "AIzaabcdefghijklmnopqrstuvwxyz123456789",
        "ya29.abcdefghijklmnopqrstuvwxyz123456789",
        "atk_abcdefghijklmnopqrstuvwxyz123456789",
        "sk-proj-abcdefghijklmnopqrstuvwxyz123456789",
        "github_pat_abcdefghijklmnopqrstuvwxyz123456789",
        "ghp_abcdefghijklmnopqrstuvwxyz123456789",
        "xoxb-abcdefghijklmnopqrstuvwxyz123456789",
        "glpat-abcdefghijklmnopqrstuvwxyz123456789",
        "tvly-abcdefghijklmnopqrstuvwxyz123456789",
        concat!("rk_", "live_abcdefghijklmnopqrstuvwxyz123456789"),
    ] {
        assert_eq!(
            secret_value_detector(token),
            Some("bare_secret_token"),
            "missed {token}"
        );
    }
}

#[test]
fn secret_value_detection_preserves_authentication_documentation() {
    for prose in [
        "Bearer authentication is supported for remote clients.",
        "Set the Authorization header before making a request.",
        "Authorization providers can enforce scopes.",
        "curl -H 'Authorization: Bearer <token>' https://example.com",
        "curl -H 'Authorization: Bearer ${ACCESS_TOKEN}' https://example.com",
    ] {
        assert!(
            !value_contains_secret(prose),
            "public documentation was classified as a secret: {prose}"
        );
    }
}

#[test]
fn url_credential_detection_preserves_documented_placeholders() {
    for example in [
        "https://<username>:<password>@example.com",
        "https://${USER}:${PASSWORD}@example.com",
        "https://{{ username }}:{{ password }}@example.com",
        r#"https://component.com","label":{"@example.com"#,
    ] {
        assert_eq!(secret_value_detector(example), None, "rejected {example}");
    }
}

#[test]
fn value_is_absolute_local_path_matches_home_and_windows_paths() {
    assert!(value_is_absolute_local_path("/home/jacob/workspace"));
    assert!(value_is_absolute_local_path(r"C:\Users\jacob"));
    assert!(!value_is_absolute_local_path("https://example.com/home/"));
    assert!(!value_is_absolute_local_path(
        "see https://example.com/home/docs and https://example.com/etc/hosts"
    ));
    assert!(value_is_absolute_local_path(
        "failed while reading /home/jacob/workspace"
    ));
}

#[test]
fn raw_dotenv_assignment_matches_upper_snake_case_keys() {
    assert!(raw_dotenv_assignment("API_KEY=abc123"));
    assert!(!raw_dotenv_assignment("just a sentence = not env"));
}

#[test]
fn dotenv_detection_requires_a_secret_like_key() {
    for assignment in [
        "PORT=3000",
        "DEBUG=true",
        "HOST=127.0.0.1",
        "FASTMCP_TRANSPORT=http",
        "FEATURE_AUTH=true",
    ] {
        assert!(
            !raw_dotenv_assignment(assignment),
            "benign configuration was classified as a secret: {assignment}"
        );
        assert!(!value_contains_secret(assignment));
    }
}

#[test]
fn assignment_detection_preserves_uri_schemes_and_variable_references() {
    for example in ["secret://provider/path", "token=jwt_access_token"] {
        assert_eq!(secret_value_detector(example), None, "rejected {example}");
    }
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
fn looks_like_bare_cookie_string_matches_credential_shaped_cookie_values() {
    assert!(looks_like_bare_cookie_string(
        "sessionid=9f8a7b6c5d4e3f2a1b0c; Path=/; HttpOnly"
    ));
    assert!(looks_like_bare_cookie_string(
        "csrftoken=abcdef0123456789abcdef01234567; SameSite=Lax"
    ));
    assert!(!value_contains_secret("Cookie: theme=dark; mode=compact"));
    assert!(!looks_like_bare_cookie_string("theme=dark; mode=compact"));
    assert!(!looks_like_bare_cookie_string("a=1; b=2"));
    assert!(!looks_like_bare_cookie_string(
        "Alice went to the store; Bob stayed home"
    ));
    assert!(!looks_like_bare_cookie_string("just one segment"));
}

#[test]
fn opaque_token_entropy_requires_credential_shaped_field() {
    assert!(field_is_opaque_token_context("gitlab_token"));
    assert!(field_is_opaque_token_context("gitea_deploy_token"));
    assert!(field_is_opaque_token_context("oauth_client_secret"));
    assert!(!field_is_opaque_token_context("gitlab_identifier"));
    assert!(!field_is_opaque_token_context("oauth_status"));
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

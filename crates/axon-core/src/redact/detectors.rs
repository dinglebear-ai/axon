//! Field-name and free-text secret detectors shared by [`super::Redactor`]
//! and any crate-local payload validator that needs to agree on what a
//! secret looks like (e.g. `axon-vectors`'s vector payload validator).

use regex::Regex;
use std::sync::LazyLock;

#[path = "detectors/spans.rs"]
mod spans;
use spans::*;
pub(super) use spans::{
    redact_operational_secret_spans, redact_retrievable_body_secret_spans, redact_secret_spans,
};
#[path = "detectors/context.rs"]
mod context;
pub use context::{
    field_is_opaque_token_context, last_field_segment, raw_dotenv_assignment,
    value_is_absolute_local_path, value_is_high_entropy_token,
};
use context::{is_documented_assignment_placeholder, is_documented_secret_placeholder};
#[path = "detectors/vocabulary.rs"]
mod vocabulary;
pub use vocabulary::{
    BARE_SECRET_TOKEN_PREFIXES, FORBIDDEN_FIELD_FRAGMENTS, FORBIDDEN_VALUE_FRAGMENTS,
    SECRET_LIKE_FIELD_FRAGMENTS,
};

/// Field names that are secret-shaped but not hard-forbidden. Non-fatal:
/// callers typically drop the field rather than reject the whole write.
pub fn secret_like_field_name(field: &str) -> bool {
    let normalized = normalize_field_name(field);
    if non_secret_security_field(&normalized) {
        return false;
    }
    matches!(
        normalized.as_str(),
        "token"
            | "secret"
            | "credential"
            | "credentials"
            | "password"
            | "passwd"
            | "api_key"
            | "apikey"
            | "access_token"
            | "refresh_token"
            | "id_token"
            | "private_key"
            | "client_secret"
            | "authorization"
            | "proxy_authorization"
    ) || normalized.ends_with("_token")
        || normalized.ends_with("_secret")
        || normalized.ends_with("_password")
        || normalized.ends_with("_passwd")
        || normalized.ends_with("_api_key")
        || normalized.ends_with("_apikey")
        || normalized.ends_with("_private_key")
        || normalized.ends_with("_credentials")
        || normalized.ends_with("_credential")
}

pub fn normalize_field_name(field: &str) -> String {
    let mut normalized = String::with_capacity(field.len() + 4);
    let mut previous_lower_or_digit = false;
    for ch in field.chars() {
        if matches!(ch, '-' | '.' | ' ') {
            if !normalized.ends_with('_') {
                normalized.push('_');
            }
            previous_lower_or_digit = false;
            continue;
        }
        if ch.is_ascii_uppercase() && previous_lower_or_digit && !normalized.ends_with('_') {
            normalized.push('_');
        }
        normalized.push(ch.to_ascii_lowercase());
        previous_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    normalized
}

fn non_secret_security_field(field: &str) -> bool {
    field.ends_with("_count")
        || field.ends_with("_estimate")
        || field.ends_with("_policy")
        || field.ends_with("_status")
        || field.ends_with("_type")
        || field.ends_with("_enabled")
        || field.ends_with("_identifier")
        || matches!(
            field,
            "tokenizer"
                | "tokenization"
                | "token_budget"
                | "page_token"
                | "next_page_token"
                | "continuation_token"
                | "pagination_token"
                | "cursor_token"
        )
}

/// Field names that are hard-forbidden because they directly carry raw
/// credential material. Security-related descriptive fields are not fatal.
pub fn forbidden_field_name(field: &str) -> bool {
    let normalized = normalize_field_name(field);
    matches!(
        normalized.as_str(),
        "raw_auth"
            | "raw_auth_header"
            | "raw_auth_headers"
            | "auth_header"
            | "authorization"
            | "authorization_header"
            | "proxy_authorization"
            | "proxy_authorization_header"
            | "cookie"
            | "cookies"
            | "cookie_header"
            | "set_cookie"
            | "set_cookie_header"
            | "raw_cookie"
            | "raw_env"
            | "raw_env_value"
            | "env_value"
            | "absolute_home"
            | "home_path"
            | "raw_html"
            | "html_blob"
            | "adapter_response"
            | "adapter_response_blob"
            | "response_blob"
    )
}

/// Whether a free-text string carries a secret-shaped value.
pub fn value_contains_secret(value: &str) -> bool {
    secret_value_detector(value).is_some()
}

/// Return a stable, value-free detector identifier for a secret-bearing
/// string. Callers may log this identifier, but must never log the matched
/// value.
pub fn secret_value_detector(value: &str) -> Option<&'static str> {
    if contains_contextual_authorization_value(value) {
        Some("authorization_value")
    } else if contains_standalone_bearer_value(value) {
        Some("bearer_value")
    } else if contains_contextual_cookie_value(value) || looks_like_bare_cookie_string(value) {
        Some("cookie_value")
    } else if contains_secret_assignment(value) {
        Some("secret_assignment")
    } else if contains_bare_secret_token(value) {
        Some("bare_secret_token")
    } else if contains_pem_private_key_block(value) {
        Some("pem_private_key")
    } else if contains_url_embedded_credentials(value) {
        Some("url_credentials")
    } else {
        None
    }
}

/// High-confidence detector for retrievable document/chunk bodies. This is
/// intentionally narrower than `secret_value_detector`: documentation often
/// contains low-entropy credential syntax such as `TOKEN=abc123` or
/// `user:password@localhost`. Operational egress still uses the conservative
/// detector; vector bodies only hard-skip when concrete credential evidence is
/// strong enough to justify losing searchable content.
pub fn retrievable_body_secret_detector(value: &str) -> Option<&'static str> {
    if contains_contextual_authorization_value(value) {
        Some("authorization_value")
    } else if contains_standalone_bearer_value(value) {
        Some("bearer_value")
    } else if contains_contextual_cookie_value(value) || looks_like_bare_cookie_string(value) {
        Some("cookie_value")
    } else if contains_high_confidence_secret_assignment(value) {
        Some("secret_assignment")
    } else if contains_bare_secret_token(value) {
        Some("bare_secret_token")
    } else if contains_pem_private_key_block(value) {
        Some("pem_private_key")
    } else if contains_high_confidence_url_credentials(value) {
        Some("url_credentials")
    } else {
        None
    }
}

fn contains_high_confidence_secret_assignment(value: &str) -> bool {
    SECRET_ASSIGNMENT_RE.captures_iter(value).any(|captures| {
        let Some(key) = captures.name("key").map(|matched| matched.as_str()) else {
            return false;
        };
        let Some(raw_value) = captures.name("value").map(|matched| matched.as_str()) else {
            return false;
        };
        secret_assignment_is_high_confidence(key, raw_value)
    })
}

fn secret_assignment_is_high_confidence(key: &str, raw_value: &str) -> bool {
    if is_authorization_field(key)
        || !secret_like_field_name(key)
        || is_documented_assignment_placeholder(key, raw_value)
    {
        return false;
    }
    let candidate = assignment_candidate(raw_value);
    if is_documented_body_example_value(candidate) {
        return false;
    }
    contains_bare_secret_token(candidate)
        || looks_like_jwt(candidate)
        || value_is_high_entropy_token(candidate)
        || contains_high_confidence_url_credentials(candidate)
        || ({
            let normalized = normalize_field_name(key);
            (matches!(normalized.as_str(), "password" | "passwd")
                || normalized.ends_with("_password")
                || normalized.ends_with("_passwd"))
                && candidate.len() >= 12
        })
}

fn assignment_candidate(value: &str) -> &str {
    value
        .trim()
        .trim_matches(|ch: char| ch as u32 == 39 || matches!(ch, '"' | ',' | ';'))
}

fn is_documented_body_example_value(value: &str) -> bool {
    if is_documented_secret_placeholder(value) {
        return true;
    }
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "user"
            | "username"
            | "pass"
            | "password"
            | "secret"
            | "token"
            | "abc123"
            | "hunter2"
            | "changeme"
            | "example"
            | "test"
            | "demo"
            | "secret-token"
    )
}

fn contains_high_confidence_url_credentials(value: &str) -> bool {
    URL_CREDENTIALS_RE.captures_iter(value).any(|captures| {
        let password = captures
            .name("password")
            .map_or("", |matched| matched.as_str());
        url_password_is_high_confidence(password)
    })
}

fn url_password_is_high_confidence(password: &str) -> bool {
    if is_documented_body_example_value(password) {
        return false;
    }
    contains_bare_secret_token(password)
        || looks_like_jwt(password)
        || value_is_high_entropy_token(password)
        || password.len() >= 8
}

fn contains_standalone_bearer_value(value: &str) -> bool {
    STANDALONE_BEARER_VALUE_RE
        .captures_iter(value)
        .filter_map(|captures| captures.name("value"))
        .any(|matched| {
            let candidate = matched.as_str();
            !is_documented_secret_placeholder(candidate)
                && (value_is_high_entropy_token(candidate)
                    || KNOWN_SECRET_TOKEN_RE.is_match(candidate))
        })
}

fn contains_contextual_cookie_value(value: &str) -> bool {
    COOKIE_VALUE_RE.captures_iter(value).any(|captures| {
        captures
            .name("value")
            .is_some_and(|matched| cookie_value_is_secret(matched.as_str()))
    })
}

fn cookie_value_is_secret(value: &str) -> bool {
    if is_documented_secret_placeholder(value) {
        return false;
    }
    value.split(';').map(str::trim).any(|segment| {
        let Some((key, raw_value)) = segment.split_once('=') else {
            return false;
        };
        let raw_value = raw_value.trim();
        if raw_value.is_empty() || is_documented_secret_placeholder(raw_value) {
            return false;
        }
        let key = normalize_field_name(key.trim());
        let credential_cookie = matches!(
            key.as_str(),
            "session"
                | "sessionid"
                | "session_id"
                | "session_token"
                | "csrf"
                | "csrftoken"
                | "csrf_token"
                | "xsrf"
                | "xsrf_token"
                | "x_csrf_token"
        );
        credential_cookie
            || secret_like_field_name(&key)
            || contains_bare_secret_token(raw_value)
            || value_is_high_entropy_token(raw_value)
    })
}

/// Authorization syntax alone is not secret evidence; the value must look
/// credential-shaped. Basic auth is always credential-bearing once present.
pub fn contains_contextual_authorization_value(value: &str) -> bool {
    AUTHORIZATION_VALUE_RE.captures_iter(value).any(|captures| {
        let raw_value = authorization_capture_value(&captures).unwrap_or_default();
        let prefix = captures
            .name("prefix")
            .map_or("", |matched| matched.as_str());
        authorization_value_is_secret(prefix, raw_value)
    })
}

fn authorization_value_is_secret(prefix: &str, value: &str) -> bool {
    if is_documented_secret_placeholder(value) {
        return false;
    }
    if prefix.to_ascii_lowercase().contains("basic ") {
        return !value.is_empty();
    }
    contains_bare_secret_token(value) || value_is_high_entropy_token(value) || looks_like_jwt(value)
}

fn looks_like_jwt(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts.iter().all(|part| {
            part.len() >= 8
                && part
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
        })
}

fn contains_secret_assignment(value: &str) -> bool {
    SECRET_ASSIGNMENT_RE.captures_iter(value).any(|captures| {
        let Some(key) = captures.name("key").map(|matched| matched.as_str()) else {
            return false;
        };
        let Some(raw_value) = captures.name("value").map(|matched| matched.as_str()) else {
            return false;
        };
        !is_authorization_field(key)
            && secret_like_field_name(key)
            && !is_documented_assignment_placeholder(key, raw_value)
    })
}

/// Whether `value` contains a PEM-encoded private-key block
/// (`-----BEGIN ... PRIVATE KEY-----`) — RSA/EC/DSA/OpenSSH/PKCS8 keys all
/// share this header shape regardless of algorithm label.
pub fn contains_pem_private_key_block(value: &str) -> bool {
    static PEM_PRIVATE_KEY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----")
            .expect("pem private key regex is valid")
    });
    PEM_PRIVATE_KEY_RE.is_match(value)
}

/// Whether `value` contains a URL authority with a non-empty
/// username **and** password (`scheme://user:pass@host`). A bare username
/// with no password (`https://user@example.com`) is not flagged — that is a
/// common non-secret pattern (e.g. git remotes) the contract's "non-empty
/// username and password authority parts" wording excludes.
pub fn contains_url_embedded_credentials(value: &str) -> bool {
    URL_CREDENTIALS_RE.captures_iter(value).any(|captures| {
        let username = captures
            .name("username")
            .map(|matched| matched.as_str())
            .unwrap_or_default();
        let password = captures
            .name("password")
            .map(|matched| matched.as_str())
            .unwrap_or_default();
        !is_documented_secret_placeholder(username)
            && !is_documented_secret_placeholder(password)
            && !is_conventional_url_credentials_placeholder(username, password)
    })
}

fn is_conventional_url_credentials_placeholder(username: &str, password: &str) -> bool {
    let username = username.to_ascii_lowercase();
    let password = password.to_ascii_lowercase();
    // Keep this fail-closed allowlist exact. These pairs are conventional
    // documentation examples; arbitrary low-entropy credentials (including
    // `user:pass`) must still be classified and rejected.
    matches!(
        (username.as_str(), password.as_str()),
        ("user" | "username", "password") | ("gw", "pw") | ("readonly", "pass")
    )
}

/// Whether `value` looks like a bare (unlabeled) `Cookie`/`Set-Cookie`
/// header value: two or more `;`-separated segments, each either a
/// `key=value` pair or a bare attribute flag (`HttpOnly`, `Secure`, …), with
/// at least one value long enough to look like a session identifier (16+
/// chars). The length floor bounds false positives on short, clearly
/// non-secret `key=value; key2=value2` text (e.g. query-string-shaped
/// examples in docs) while still catching real cookie strings.
pub fn looks_like_bare_cookie_string(value: &str) -> bool {
    let segments: Vec<&str> = value
        .split(';')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.len() < 2 {
        return false;
    }
    let mut kv_count = 0usize;
    let mut has_long_value = false;
    for segment in &segments {
        if let Some((key, val)) = segment.split_once('=') {
            let key_ok = !key.is_empty()
                && !key.contains(char::is_whitespace)
                && key
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'));
            let val_ok = !val.is_empty() && !val.contains(char::is_whitespace);
            if !key_ok || !val_ok {
                return false;
            }
            kv_count += 1;
            if val.len() >= 16 {
                has_long_value = true;
            }
        } else if !segment.chars().all(|ch| ch.is_ascii_alphanumeric()) {
            // Not a `key=value` pair and not a bare alnum flag (HttpOnly,
            // Secure, …) — this isn't cookie-shaped text at all.
            return false;
        }
    }
    kv_count >= 1 && has_long_value
}

/// Whether `value` contains a bare secret token (`sk-...`, `ghp_...`, …) with
/// no surrounding marker (`KEY=`/`Authorization:`).
pub fn contains_bare_secret_token(value: &str) -> bool {
    BARE_SECRET_TOKEN_PREFIXES
        .iter()
        .any(|prefix| contains_bare_secret_token_with_prefix(value, prefix))
}

fn contains_bare_secret_token_with_prefix(value: &str, prefix: &str) -> bool {
    let mut search_start = 0;
    while let Some(relative_index) = value[search_start..].find(prefix) {
        let index = search_start + relative_index;
        let rest_start = index + prefix.len();
        if token_start_boundary(value, index) && token_body_len(&value[rest_start..]) >= 20 {
            return true;
        }
        search_start = rest_start;
    }
    false
}

fn token_start_boundary(value: &str, index: usize) -> bool {
    value[..index]
        .chars()
        .next_back()
        .is_none_or(|ch| !is_token_char(ch))
}

fn token_body_len(value: &str) -> usize {
    value.chars().take_while(|ch| is_token_char(*ch)).count()
}

fn is_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')
}

#[cfg(test)]
#[path = "detectors_tests.rs"]
mod tests;

//! Field-name and free-text secret detectors shared by [`super::Redactor`]
//! and any crate-local payload validator that needs to agree on what a
//! secret looks like (e.g. `axon-vectors`'s vector payload validator).

/// Field names that are secret-shaped but not hard-forbidden. Non-fatal:
/// callers typically drop the field rather than reject the whole write.
pub fn secret_like_field_name(field: &str) -> bool {
    let normalized = field.to_ascii_lowercase();
    SECRET_LIKE_FIELD_FRAGMENTS
        .iter()
        .any(|fragment| normalized.contains(fragment))
        || normalized.ends_with("_token")
        || normalized == "authorization"
        || normalized == "proxy-authorization"
}

/// Field names that are hard-forbidden — a write carrying one of these must
/// fail closed rather than silently drop or scrub the field.
pub fn forbidden_field_name(field: &str) -> bool {
    let normalized = field.to_ascii_lowercase();
    FORBIDDEN_FIELD_FRAGMENTS
        .iter()
        .any(|fragment| normalized.contains(fragment))
}

/// Whether a free-text string carries a secret-shaped value.
pub fn value_contains_secret(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    FORBIDDEN_VALUE_FRAGMENTS
        .iter()
        .any(|fragment| normalized.contains(fragment))
        || raw_dotenv_assignment(value)
        || contains_bare_secret_token(value)
        || normalized.contains("adapter_response")
}

/// Whether a free-text value looks like an absolute local filesystem path
/// (`/home/...`, `~/...`, `C:\...`, …).
pub fn value_is_absolute_local_path(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    let trimmed = value.trim();
    if normalized.starts_with("http://")
        || normalized.starts_with("https://")
        || normalized.starts_with("local-code://")
    {
        return false;
    }
    normalized.contains("/home/")
        || normalized.contains("/users/")
        || normalized.contains("/tmp/")
        || normalized.contains("/mnt/")
        || normalized.contains("/var/")
        || normalized.contains("/etc/")
        || normalized.contains("/root/")
        || trimmed.starts_with('~')
        || trimmed.starts_with("\\\\")
        || (trimmed.len() >= 3
            && trimmed.as_bytes()[0].is_ascii_alphabetic()
            && trimmed.as_bytes()[1] == b':'
            && matches!(trimmed.as_bytes()[2], b'\\' | b'/'))
}

/// Whether `value` contains a line shaped like a raw `.env` assignment
/// (`KEY=value`, all-caps key).
pub fn raw_dotenv_assignment(value: &str) -> bool {
    value.lines().any(|line| {
        let line = line.trim();
        let Some((key, raw_value)) = line.split_once('=') else {
            return false;
        };
        let key = key.trim();
        !key.is_empty()
            && !raw_value.trim().is_empty()
            && key
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
            && key
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_uppercase() || ch == '_')
    })
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

pub const FORBIDDEN_FIELD_FRAGMENTS: &[&str] = &[
    "raw_auth",
    "auth_header",
    "authorization",
    "cookie",
    "api_key",
    "apikey",
    "secret",
    "raw_env",
    "env_value",
    "absolute_home",
    "home_path",
    "raw_html",
    "html_blob",
    "adapter_response",
    "response_blob",
];

/// Field-name fragments that classify as `sensitive` and are dropped
/// non-fatally. Broader than [`FORBIDDEN_FIELD_FRAGMENTS`] (which is fatal).
pub const SECRET_LIKE_FIELD_FRAGMENTS: &[&str] = &[
    "secret",
    "credential",
    "password",
    "api_key",
    "apikey",
    "access_token",
    "refresh_token",
    "id_token",
    "private_key",
    "client_secret",
];

pub const FORBIDDEN_VALUE_FRAGMENTS: &[&str] = &[
    "authorization:",
    "proxy-authorization:",
    "bearer ",
    "cookie:",
    "set-cookie:",
    "api_key=",
    "apikey=",
    "api-key:",
    "x-api-key:",
    "access_token=",
    "refresh_token=",
    "secret_key=",
    "token=",
];

pub const BARE_SECRET_TOKEN_PREFIXES: &[&str] = &[
    "sk-proj-",
    "github_pat_",
    "sk-",
    "sk_",
    "ghp_",
    "xoxb-",
    "xoxp-",
    "glpat-",
];

#[cfg(test)]
#[path = "detectors_tests.rs"]
mod tests;

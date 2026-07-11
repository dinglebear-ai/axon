//! Field-name and free-text secret detectors shared by [`super::Redactor`]
//! and any crate-local payload validator that needs to agree on what a
//! secret looks like (e.g. `axon-vectors`'s vector payload validator).

use regex::Regex;
use std::sync::LazyLock;

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
        || contains_pem_private_key_block(value)
        || contains_url_embedded_credentials(value)
        || looks_like_bare_cookie_string(value)
        || normalized.contains("adapter_response")
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
    static URL_CREDENTIALS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"[A-Za-z][A-Za-z0-9+.\-]*://[^\s/:@]+:[^\s/@]+@[^\s/]+")
            .expect("url credentials regex is valid")
    });
    URL_CREDENTIALS_RE.is_match(value)
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

/// Field-name fragments that put a value in "opaque token" context per the
/// contract: Gitea/GitLab/OAuth-style opaque tokens carry no fixed prefix,
/// so they are classified by key/path context plus value entropy rather
/// than a prefix match.
pub const OPAQUE_TOKEN_FIELD_CONTEXT_FRAGMENTS: &[&str] = &[
    "token",
    "secret",
    "gitlab",
    "gitea",
    "oauth",
    "deploy_token",
];

/// Whether `field` name context marks its value as opaque-token-shaped.
pub fn field_is_opaque_token_context(field: &str) -> bool {
    let normalized = field.to_ascii_lowercase();
    OPAQUE_TOKEN_FIELD_CONTEXT_FRAGMENTS
        .iter()
        .any(|fragment| normalized.contains(fragment))
}

/// Whether `value` is shaped like an opaque secret token: long enough,
/// token-charset only, and high Shannon entropy. This is a **secondary**
/// signal — per the contract, entropy is only used alongside key/path
/// context ([`field_is_opaque_token_context`]), never on its own.
pub fn value_is_high_entropy_token(value: &str) -> bool {
    let trimmed = value.trim();
    const MIN_LEN: usize = 20;
    if trimmed.len() < MIN_LEN
        || !trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return false;
    }
    super::shannon_entropy_bits(trimmed) >= super::MIN_ENTROPY_BITS
}

/// Last `.`-delimited segment of a redaction field path (e.g.
/// `metadata.gitlab_token` -> `gitlab_token`), used to check field-name
/// context without matching on ancestor path segments.
pub fn last_field_segment(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
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
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "xoxb-",
    "xoxp-",
    "glpat-",
];

#[cfg(test)]
#[path = "detectors_tests.rs"]
mod tests;

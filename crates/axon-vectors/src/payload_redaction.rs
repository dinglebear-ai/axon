//! Redaction guardrails for vector payload metadata.

use serde_json::Value;

use crate::payload::VectorPayloadValidationError;

pub(crate) fn forbidden_field_name(field: &str) -> bool {
    let normalized = field.to_ascii_lowercase();
    FORBIDDEN_FIELD_FRAGMENTS
        .iter()
        .any(|fragment| normalized.contains(fragment))
}

/// Field names that are secret-shaped but not in the hard forbidden-field list
/// (which trips a fatal `ForbiddenField`). The [`crate::redactor::Redactor`]
/// drops these; the payload validator does not, so redaction runs first.
pub(crate) fn secret_like_field_name(field: &str) -> bool {
    let normalized = field.to_ascii_lowercase();
    SECRET_LIKE_FIELD_FRAGMENTS
        .iter()
        .any(|fragment| normalized.contains(fragment))
        || normalized.ends_with("_token")
        || normalized == "authorization"
        || normalized == "proxy-authorization"
}

/// Whether a free-text string carries a secret-shaped value. Reuses the same
/// value detectors the payload validator applies to `chunk_text`, so the
/// redactor and validator agree on what a secret looks like.
pub(crate) fn value_contains_secret(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    FORBIDDEN_VALUE_FRAGMENTS
        .iter()
        .any(|fragment| normalized.contains(fragment))
        || raw_dotenv_assignment(value)
        || contains_bare_secret_token(value)
        || normalized.contains("adapter_response")
}

pub(crate) fn validate_forbidden_value(
    path: &str,
    value: &Value,
) -> Result<(), VectorPayloadValidationError> {
    match value {
        Value::String(value) if forbidden_string_value(path, value) => {
            Err(VectorPayloadValidationError::ForbiddenValue {
                field: path.to_string(),
            })
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                validate_forbidden_value(&format!("{path}[{index}]"), value)?;
            }
            Ok(())
        }
        Value::Object(object) => {
            if adapter_response_blob(object) {
                return Err(VectorPayloadValidationError::ForbiddenValue {
                    field: path.to_string(),
                });
            }
            for (field, value) in object {
                let child_path = format!("{path}.{field}");
                if forbidden_field_name(field) {
                    return Err(VectorPayloadValidationError::ForbiddenValue { field: child_path });
                }
                validate_forbidden_value(&child_path, value)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn forbidden_string_value(path: &str, value: &str) -> bool {
    if BODY_TEXT_FIELDS.contains(&path) {
        return forbidden_body_text_value(value);
    }
    let normalized = value.to_ascii_lowercase();
    FORBIDDEN_VALUE_FRAGMENTS
        .iter()
        .any(|fragment| normalized.contains(fragment))
        || raw_dotenv_assignment(value)
        || contains_bare_secret_token(value)
        || absolute_local_path(path, value)
        || raw_html_blob(&normalized)
        || normalized.contains("adapter_response")
}

fn forbidden_body_text_value(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    FORBIDDEN_VALUE_FRAGMENTS
        .iter()
        .any(|fragment| normalized.contains(fragment))
        || raw_dotenv_assignment(value)
        || contains_bare_secret_token(value)
        || normalized.contains("adapter_response")
}

fn raw_dotenv_assignment(value: &str) -> bool {
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

fn contains_bare_secret_token(value: &str) -> bool {
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

/// Whether a free-text value looks like an absolute local filesystem path
/// (`/home/...`, `~/...`, `C:\...`, …). Used by the [`crate::redactor::Redactor`]
/// to redact sensitive local paths when the surface does not allow them.
pub(crate) fn value_is_absolute_local_path(value: &str) -> bool {
    absolute_local_path("", value)
}

fn absolute_local_path(_path: &str, value: &str) -> bool {
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

fn raw_html_blob(normalized: &str) -> bool {
    let trimmed = normalized.trim_start();
    trimmed.starts_with("<!doctype html")
        || trimmed.starts_with("<html")
        || (normalized.contains("<html") && normalized.contains("</html>"))
        || (normalized.contains("<body") && normalized.contains("</body>"))
}

fn adapter_response_blob(object: &serde_json::Map<String, Value>) -> bool {
    let has_status = object.contains_key("status") || object.contains_key("status_code");
    let has_headers = object.contains_key("headers");
    let has_body = object.contains_key("body")
        || object.contains_key("raw_body")
        || object.contains_key("response_body");
    has_status && has_headers && has_body
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

/// Field-name fragments that classify as `sensitive` and are dropped by the
/// redactor. Broader than [`FORBIDDEN_FIELD_FRAGMENTS`] (which is fatal): these
/// are scrubbed non-fatally so an adapter that stamps e.g. `access_token`
/// metadata drops that field rather than failing the whole index.
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

const BODY_TEXT_FIELDS: &[&str] = &["chunk_text"];

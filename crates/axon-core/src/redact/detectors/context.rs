use super::secret_like_field_name;

/// Whether `field` name context marks its value as opaque-token-shaped.
pub fn field_is_opaque_token_context(field: &str) -> bool {
    secret_like_field_name(field)
}

/// Whether `value` is shaped like an opaque secret token.
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
    super::super::shannon_entropy_bits(trimmed) >= super::super::MIN_ENTROPY_BITS
}

pub fn last_field_segment(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

pub fn value_is_absolute_local_path(value: &str) -> bool {
    value.split_whitespace().any(|token| {
        let trimmed = token.trim_matches(|ch: char| {
            matches!(ch, '"' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';')
        });
        let normalized = trimmed.to_ascii_lowercase();
        if normalized.starts_with("http://")
            || normalized.starts_with("https://")
            || normalized.starts_with("local-code://")
        {
            return false;
        }
        normalized.starts_with("/home/")
            || normalized.starts_with("/users/")
            || normalized.starts_with("/tmp/")
            || normalized.starts_with("/mnt/")
            || normalized.starts_with("/var/")
            || normalized.starts_with("/etc/")
            || normalized.starts_with("/root/")
            || trimmed.starts_with("~/")
            || trimmed.starts_with("\\")
            || (trimmed.len() >= 3
                && trimmed.as_bytes()[0].is_ascii_alphabetic()
                && trimmed.as_bytes()[1] == b':'
                && (trimmed.as_bytes()[2] == 92 || trimmed.as_bytes()[2] == b'/'))
    })
}

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
            && secret_like_field_name(key)
            && !is_documented_assignment_placeholder(key, raw_value)
    })
}

pub(super) fn is_documented_secret_placeholder(value: &str) -> bool {
    let trimmed = value
        .trim()
        .trim_matches(|ch: char| matches!(ch, '\'' | '"' | '`' | ',' | ';'));
    if trimmed.is_empty() || trimmed == "..." {
        return true;
    }
    if (trimmed.starts_with('<') && trimmed.ends_with('>'))
        || (trimmed.starts_with("${") && trimmed.ends_with('}'))
        || (trimmed.starts_with("{{") && trimmed.ends_with("}}"))
    {
        return true;
    }
    let normalized = trimmed.to_ascii_lowercase();
    normalized == "[redacted]"
        || normalized.starts_with("your-")
        || normalized.starts_with("your_")
        || normalized.starts_with("replace-")
        || normalized.starts_with("replace_")
        || normalized == "placeholder"
}

pub(super) fn is_documented_assignment_placeholder(key: &str, value: &str) -> bool {
    if is_documented_secret_placeholder(value) || value.trim_start().starts_with("//") {
        return true;
    }
    if !secret_like_field_name(key) {
        return false;
    }
    let normalized = value
        .trim()
        .trim_matches(|ch: char| matches!(ch, '\'' | '"' | '`' | ',' | ';'))
        .to_ascii_lowercase();
    [
        "access_token",
        "refresh_token",
        "id_token",
        "api_key",
        "secret_key",
        "client_secret",
        "password",
    ]
    .iter()
    .any(|name| normalized == *name || normalized.ends_with(&format!("_{name}")))
}

use super::*;

pub(in crate::redact) fn redact_operational_secret_spans(value: &str) -> String {
    let redacted = redact_secret_spans(value);
    let authorization_redacted = AUTHORIZATION_VALUE_RE
        .replace_all(&redacted, |captures: &regex::Captures| {
            let raw_value = authorization_capture_value(captures).unwrap_or_default();
            if is_documented_secret_placeholder(raw_value) {
                captures[0].to_string()
            } else {
                format!(
                    "{}{}",
                    captures
                        .name("prefix")
                        .map_or("", |matched| matched.as_str()),
                    super::super::REDACTION_PLACEHOLDER
                )
            }
        })
        .into_owned();
    let identifiers_redacted = OPERATIONAL_CREDENTIAL_IDENTIFIER_RE
        .replace_all(&authorization_redacted, super::super::REDACTION_PLACEHOLDER)
        .into_owned();
    OPERATIONAL_JWT_RE
        .replace_all(&identifiers_redacted, super::super::REDACTION_PLACEHOLDER)
        .into_owned()
}

fn redact_secret_spans_with_policy(value: &str, retrievable_body: bool) -> String {
    let bearer_redacted =
        STANDALONE_BEARER_VALUE_RE.replace_all(value, |captures: &regex::Captures| {
            let candidate = captures
                .name("value")
                .map_or("", |matched| matched.as_str());
            if is_documented_secret_placeholder(candidate)
                || (!value_is_high_entropy_token(candidate)
                    && !KNOWN_SECRET_TOKEN_RE.is_match(candidate))
            {
                captures[0].to_string()
            } else {
                format!(
                    "{}{}",
                    captures
                        .name("prefix")
                        .map_or("", |matched| matched.as_str()),
                    super::super::REDACTION_PLACEHOLDER
                )
            }
        });
    let authorization_redacted =
        AUTHORIZATION_VALUE_RE.replace_all(&bearer_redacted, |captures: &regex::Captures| {
            let raw_value = authorization_capture_value(captures).unwrap_or_default();
            let prefix = captures
                .name("prefix")
                .map_or("", |matched| matched.as_str());
            if authorization_value_is_secret(prefix, raw_value) {
                format!("{prefix}{}", super::super::REDACTION_PLACEHOLDER)
            } else {
                captures[0].to_string()
            }
        });
    let cookie_redacted =
        COOKIE_VALUE_RE.replace_all(&authorization_redacted, |captures: &regex::Captures| {
            let raw_value = captures
                .name("value")
                .map_or("", |matched| matched.as_str());
            if cookie_value_is_secret(raw_value) {
                format!(
                    "{}{}",
                    captures
                        .name("prefix")
                        .map_or("", |matched| matched.as_str()),
                    super::super::REDACTION_PLACEHOLDER
                )
            } else {
                captures[0].to_string()
            }
        });
    let assignments_redacted =
        SECRET_ASSIGNMENT_RE.replace_all(&cookie_redacted, |captures: &regex::Captures| {
            let key = captures.name("key").map_or("", |matched| matched.as_str());
            let raw_value = captures
                .name("value")
                .map_or("", |matched| matched.as_str());
            let should_redact = if retrievable_body {
                secret_assignment_is_high_confidence(key, raw_value)
            } else {
                !is_authorization_field(key)
                    && secret_like_field_name(key)
                    && !is_documented_assignment_placeholder(key, raw_value)
            };
            if should_redact {
                format!(
                    "{}{}",
                    captures
                        .name("prefix")
                        .map_or("", |matched| matched.as_str()),
                    super::super::REDACTION_PLACEHOLDER
                )
            } else {
                captures[0].to_string()
            }
        });
    let url_redacted =
        URL_CREDENTIALS_RE.replace_all(&assignments_redacted, |captures: &regex::Captures| {
            let username = captures
                .name("username")
                .map_or("", |matched| matched.as_str());
            let password = captures
                .name("password")
                .map_or("", |matched| matched.as_str());
            let should_redact = if retrievable_body {
                url_password_is_high_confidence(password)
            } else {
                !is_documented_secret_placeholder(username)
                    && !is_documented_secret_placeholder(password)
            };
            if !should_redact {
                return captures[0].to_string();
            }
            let matched = &captures[0];
            let Some(authority_start) = matched.find("://").map(|index| index + 3) else {
                return super::super::REDACTION_PLACEHOLDER.to_string();
            };
            let Some(at) = matched.rfind('@') else {
                return super::super::REDACTION_PLACEHOLDER.to_string();
            };
            format!(
                "{}{}{}",
                &matched[..authority_start],
                super::super::REDACTION_PLACEHOLDER,
                &matched[at..]
            )
        });
    KNOWN_SECRET_TOKEN_RE
        .replace_all(&url_redacted, super::super::REDACTION_PLACEHOLDER)
        .into_owned()
}

pub(in crate::redact) fn redact_secret_spans(value: &str) -> String {
    redact_secret_spans_with_policy(value, false)
}

pub(in crate::redact) fn redact_retrievable_body_secret_spans(value: &str) -> String {
    redact_secret_spans_with_policy(value, true)
}

pub(super) static URL_CREDENTIALS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[A-Za-z][A-Za-z0-9+.\-]*://(?P<username>[A-Za-z0-9._~!$&'()*+,;=%-]+):(?P<password>[A-Za-z0-9._~!$&'()*+,;=:%-]+)@(?P<host>[A-Za-z0-9.-]+)").expect("url credentials regex is valid")
});

static OPERATIONAL_CREDENTIAL_IDENTIFIER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b")
        .expect("operational credential identifier regex is valid")
});

static OPERATIONAL_JWT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\beyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\b")
        .expect("operational JWT regex is valid")
});

pub(super) static KNOWN_SECRET_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
          AIza[0-9A-Za-z_\-]{35}
        | ya29\.[A-Za-z0-9_\-]{20,}
        | \bsk-proj-[A-Za-z0-9_\-]{20,}
        | \bgithub_pat_[A-Za-z0-9_\-]{20,}
        | \bsk-[A-Za-z0-9_\-]{20,}
        | \bsk_[A-Za-z0-9_\-]{20,}
        | \bgh[pousr]_[A-Za-z0-9_\-]{20,}
        | \batk_[A-Za-z0-9_\-]{20,}
        | \bxox[bp]-[A-Za-z0-9_\-]{20,}
        | \bglpat-[A-Za-z0-9_\-]{20,}
        | \btvly-[A-Za-z0-9_\-]{20,}
        | \brk_(?:test|live)_[A-Za-z0-9_\-]{20,}
        ",
    )
    .expect("known secret token regex is valid")
});

pub(super) static AUTHORIZATION_VALUE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(?P<prefix>(?:proxy-)?authorization\s*[:=]\s*(?:(?:bearer|basic|token)\s+)?)(?P<value>[^\s'\";,]+)"#).expect("authorization value regex is valid")
});

pub(super) static STANDALONE_BEARER_VALUE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(?P<prefix>bearer\s+)(?P<value>[^\s'\";,]+)"#)
        .expect("standalone bearer regex is valid")
});

pub(super) static COOKIE_VALUE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?im)\b(?P<prefix>(?:set-cookie|cookie)\s*:\s*)(?P<value>[^\r\n'\"]+)"#)
        .expect("cookie value regex is valid")
});

pub(super) fn authorization_capture_value<'a>(
    captures: &'a regex::Captures<'a>,
) -> Option<&'a str> {
    captures.name("value").map(|matched| matched.as_str())
}

pub(super) fn is_authorization_field(field: &str) -> bool {
    matches!(
        normalize_field_name(field).as_str(),
        "authorization" | "proxy_authorization"
    )
}

pub(super) static SECRET_ASSIGNMENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(?P<prefix>(?P<key>[a-z_][a-z0-9_-]*)\s*[:=]\s*)(?P<value>"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|[^\s;,]+)"#).expect("secret assignment regex is valid")
});

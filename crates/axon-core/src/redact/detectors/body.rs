//! Source-aware evidence checks for retrievable bodies.

use super::*;

pub(super) fn retrievable_assignment_is_high_confidence(
    source: &str,
    captures: &regex::Captures<'_>,
) -> bool {
    let Some(key) = captures.name("key").map(|matched| matched.as_str()) else {
        return false;
    };
    if !secret_like_field_name(key) || is_authorization_field(key) {
        return false;
    }
    let Some(value_match) = captures.name("value") else {
        return false;
    };
    let raw_value = value_match.as_str();
    // A multiline capture starting at the closing delimiter of an existing
    // source string spans unrelated expressions, rather than a credential.
    if raw_value.contains(['\r', '\n'])
        && bounded_line_prefix(source, value_match.start(), 4096)
            .and_then(open_quote)
            .is_some_and(|quote| raw_value.starts_with(quote))
    {
        return false;
    }
    // A declared JavaScript variable with a call RHS contains executable syntax,
    // rather than a literal password. An unquoted dotenv value receives no exemption.
    if SOURCE_CALL_RE.is_match(raw_value)
        && bounded_line_prefix(source, captures.get(0).expect("complete match").start(), 64)
            .is_some_and(|prefix| matches!(prefix.trim(), "const" | "let" | "var"))
    {
        return false;
    }
    secret_assignment_is_high_confidence(key, raw_value)
}

// Unknown context receives no source-syntax exemption. Bound work even for
// many sensitive assignments on one huge/minified line; preserve UTF-8 slices.
fn bounded_line_prefix(source: &str, end: usize, limit: usize) -> Option<&str> {
    let mut start = end.saturating_sub(limit);
    while !source.is_char_boundary(start) {
        start += 1;
    }
    let window = &source[start..end];
    match window.rfind('\n') {
        Some(index) => Some(&window[index + 1..]),
        None if start == 0 => Some(window),
        None => None,
    }
}

fn open_quote(line: &str) -> Option<char> {
    let mut quote = None;
    let mut escaped = false;
    for ch in line.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        match quote {
            Some(open) if ch == open => quote = None,
            None if matches!(ch, '\'' | '"') => quote = Some(ch),
            _ => {}
        }
    }
    quote
}

static SOURCE_CALL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z_$][A-Za-z0-9_$]*(?:\.[A-Za-z_$][A-Za-z0-9_$]*)*\(")
        .expect("source call regex is valid")
});

/// A quoted header assertion is harmless; partial encoded key material is not.
pub(super) fn contains_pem_private_key_material(value: &str) -> bool {
    PEM_KEY_MATERIAL_RE.is_match(value)
}

// Capture the entire continuous encoded body, including an incomplete last
// line. A separate terminator preserves the source string delimiter/newline.
pub(super) static PEM_KEY_MATERIAL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)(?P<material>-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----[ \t]*(?:(?:\r?\n|\\n)[ \t]*)+[A-Za-z0-9+/]{8,}={0,2}(?:(?:(?:\r?\n|\\n)[ \t]*)+[A-Za-z0-9+/]+={0,2})*)(?P<suffix>[ \t]*(?:\r?$|\r?\n|\\n|["']))"#)
        .expect("PEM material regex is valid")
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minified_line_context_is_bounded_and_unknown_context_stays_protected() {
        let source = format!(
            "{}password=\"firstline\nsecondline\"",
            "x=benign;".repeat(50_000)
        );
        let key_start = source.find("password").unwrap();
        assert_eq!(bounded_line_prefix(&source, key_start, 64), None);
        assert_eq!(
            retrievable_body_secret_detector(&source),
            Some("secret_assignment")
        );
        let unicode = format!(
            "{}const password=readQueryParameter('password');",
            "界".repeat(200)
        );
        assert_eq!(
            bounded_line_prefix(&unicode, unicode.find("password").unwrap(), 64),
            None
        );
    }
}

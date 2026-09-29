//! Bounded public operational diagnostics.

/// Redact secrets and URL parameters before truncating at a UTF-8 boundary.
#[must_use]
pub fn public_diagnostic_text(input: &str, limit: usize) -> String {
    use super::{DefaultRedactor, RedactionContext, redact_text_checked};
    let safe = redact_text_checked(
        &DefaultRedactor::new(),
        input,
        &RedactionContext::job_event(),
    )
    .unwrap_or_else(|_| "diagnostic detail suppressed".into());
    let mut output = String::new();
    let safe = strip_url_parameters(&safe);
    for ch in safe.chars() {
        let ch = if ch.is_control() { ' ' } else { ch };
        if output.len() + ch.len_utf8() > limit {
            break;
        }
        output.push(ch);
    }
    output
}

// URL parameters often contain short credentials that token detectors cannot identify.
fn strip_url_parameters(input: &str) -> String {
    input
        .split_inclusive(char::is_whitespace)
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            let Some(start) = [lower.find("https://"), lower.find("http://")]
                .into_iter()
                .flatten()
                .min()
            else {
                return part.to_owned();
            };
            let authority_start = start + part[start..].find("://").unwrap() + 3;
            let authority_end = part[authority_start..]
                .find(['/', '?', '#'])
                .map_or(part.trim_end().len(), |n| authority_start + n);
            let mut cleaned = match part[authority_start..authority_end].rfind('@') {
                Some(n) => format!(
                    "{}[REDACTED]@{}",
                    &part[..authority_start],
                    &part[authority_start + n + 1..]
                ),
                None => part.to_owned(),
            };
            if let Some(offset) = cleaned[start..].find(['?', '#']) {
                let suffix = &cleaned[cleaned.trim_end().len()..];
                cleaned = format!("{}[REDACTED]{}", &cleaned[..start + offset], suffix);
            }
            cleaned
        })
        .collect()
}

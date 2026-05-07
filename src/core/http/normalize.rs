//! URL normalization utilities.

use std::borrow::Cow;

/// Normalize a URL by prepending `https://` when the scheme is absent and the
/// input looks like a hostname.
///
/// Returns `Cow::Borrowed` when the input already has a scheme and no
/// leading/trailing whitespace, avoiding allocation in the common case.
pub fn normalize_url(url: &str) -> Cow<'_, str> {
    let trimmed = url.trim();

    // Fast path: already has a scheme and no whitespace was trimmed — borrow.
    if trimmed.contains("://") && trimmed.len() == url.len() {
        return Cow::Borrowed(url);
    }

    // Empty after trim — borrow the trimmed slice.
    if trimmed.is_empty() {
        return Cow::Borrowed(trimmed);
    }

    // Has scheme but was trimmed — must allocate the trimmed copy.
    if trimmed.contains("://") {
        return Cow::Owned(trimmed.to_string());
    }

    let looks_like_host = trimmed.contains('.')
        || trimmed.starts_with("localhost")
        || trimmed.starts_with("127.0.0.1")
        || trimmed.starts_with("[::1]");
    let has_no_spaces = !trimmed.chars().any(char::is_whitespace);

    if looks_like_host && has_no_spaces {
        Cow::Owned(format!("https://{trimmed}"))
    } else if trimmed.len() == url.len() {
        Cow::Borrowed(url)
    } else {
        Cow::Owned(trimmed.to_string())
    }
}

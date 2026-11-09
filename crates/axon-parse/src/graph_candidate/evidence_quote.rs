//! Bound retained graph evidence without changing facts or source indexing.
const MAX_QUOTE_BYTES: usize = 4096;

pub(super) fn bounded(quote: Option<String>, name: &str) -> (Option<String>, bool, usize) {
    let Some(text) = quote else {
        return (None, false, 0);
    };
    let original = text.len();
    if original <= MAX_QUOTE_BYTES {
        return (Some(text), false, original);
    }
    let anchor = text.find(name).unwrap_or(0);
    let mut start = anchor.saturating_sub(MAX_QUOTE_BYTES / 4);
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = start.saturating_add(MAX_QUOTE_BYTES).min(original);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (Some(text[start..end].to_owned()), true, original)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_unicode_quote_is_literal_bounded_and_near_named_evidence() {
        let name = "service Actual";
        let source = format!("{}{}{}", "α".repeat(100_000), name, "β".repeat(100_000));
        let (quote, excerpted, original) = bounded(Some(source.clone()), name);
        let quote = quote.unwrap();
        assert!(excerpted);
        assert_eq!(original, source.len());
        assert!(quote.len() <= MAX_QUOTE_BYTES);
        assert!(quote.contains(name));
        assert!(source.contains(&quote));
    }
}

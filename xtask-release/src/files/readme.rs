use regex::Regex;

use super::super::{ReleaseContext, ReleaseResult};

pub(super) fn check_version_line(content: &str, expected: &str) -> ReleaseResult<()> {
    let regex = Regex::new(&format!(
        r#"(?m)^Version:\s*{}\b(?:\s+<!-- x-release-please-version -->)?\s*$"#,
        regex::escape(expected)
    ))
    .release_context("invalid README version check regex")?;
    if !regex.is_match(content) {
        release_bail!("missing 'Version: {expected}'");
    }
    Ok(())
}

/// Set the `Version:` line to `next`, dropping the `x-release-please-version`
/// marker comment (release-please no longer manages this file for the `cli`
/// component — see `release/components.toml` and `CLAUDE.md`'s Release
/// Pipeline section).
pub(super) fn replace_version_line(content: &str, next: &str) -> ReleaseResult<String> {
    // `[ \t]*$` (not `\s*$`) so the match stops at this line's own newline —
    // `\s` matches `\n` too, and a greedy `\s*$` here swallowed the blank
    // line that follows in practice, collapsing "Version: X\n\nAxon is..."
    // down to "Version: X\nAxon is...".
    let regex =
        Regex::new(r#"(?m)^Version:\s*\S+\b(?:\s+<!-- x-release-please-version -->)?[ \t]*$"#)
            .release_context("invalid README version replacement regex")?;
    if !regex.is_match(content) {
        release_bail!("missing 'Version:' line");
    }
    Ok(regex
        .replace(content, format!("Version: {next}"))
        .into_owned())
}

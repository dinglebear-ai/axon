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

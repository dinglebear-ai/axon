//! Manual write path for the `cli` component's version-bearing files —
//! release-please no longer manages it (see `release/components.toml`'s
//! `cli` entry and CLAUDE.md's Release Pipeline section). Used by
//! `cargo xtask bump-version cli`.

use regex::Regex;

use super::super::{ReleaseContext, ReleaseResult};
use super::{read_json_version, read_npm_package_lock_version, read_workspace_package_version};

/// Set `[package] version` in a Cargo manifest, touching only that table (a
/// `version = "..."` key elsewhere, e.g. under `[dependencies]`, is untouched).
pub(super) fn replace_cargo_package_version(content: &str, next: &str) -> ReleaseResult<String> {
    let mut in_package = false;
    let mut replaced = false;
    let mut output = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
        } else if in_package && trimmed.starts_with('[') {
            in_package = false;
        }
        let mut next_line = line.to_owned();
        if in_package && trimmed.starts_with("version") && trimmed.contains('"') {
            let leading = &line[..line.len() - line.trim_start().len()];
            next_line = format!(r#"{leading}version = "{next}""#);
            replaced = true;
        }
        output.push(next_line);
    }
    if !replaced {
        release_bail!("missing [package] version");
    }
    Ok(preserve_trailing_newline(content, output.join("\n")))
}

/// Set `[workspace.package] version` in a Cargo manifest, the same way as
/// [`replace_cargo_package_version`] but scoped to `[workspace.package]`.
/// A no-op (returns the content unchanged) if the manifest declares no
/// `[workspace.package] version` — matches `check_workspace_package_version`'s
/// "no-op when absent" semantics.
pub(super) fn replace_workspace_package_version(
    content: &str,
    next: &str,
) -> ReleaseResult<String> {
    if read_workspace_package_version(content)?.is_none() {
        return Ok(content.to_owned());
    }
    let mut in_workspace_package = false;
    let mut replaced = false;
    let mut output = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[workspace.package]" {
            in_workspace_package = true;
        } else if in_workspace_package && trimmed.starts_with('[') {
            in_workspace_package = false;
        }
        let mut next_line = line.to_owned();
        if in_workspace_package && trimmed.starts_with("version") && trimmed.contains('"') {
            let leading = &line[..line.len() - line.trim_start().len()];
            next_line = format!(r#"{leading}version = "{next}""#);
            replaced = true;
        }
        output.push(next_line);
    }
    if !replaced {
        release_bail!("missing [workspace.package] version");
    }
    Ok(preserve_trailing_newline(content, output.join("\n")))
}

/// Set the JSON value at `pointer` (default `/version`) to `next`.
///
/// Deliberately does NOT round-trip through `serde_json::Value` +
/// `to_string_pretty` — tried that first, and it reformatted
/// `apps/web/openapi/axon.json` wholesale (a ~16,700-line diff for a one-line
/// version bump), because serde_json's pretty-printer doesn't reproduce
/// whatever tool originally generated that file's exact formatting. Instead,
/// this validates the current value via `read_json_version` (which does
/// parse JSON, so the pointer and "is a string" checks are still real), then
/// does a literal text substitution of the exact `"KEY": "OLD"` substring
/// (KEY = pointer's last segment) — preserving every byte of surrounding
/// formatting. Requires that substring to appear exactly once; refuses to
/// guess if it's ambiguous or literally absent from the raw text (e.g. an
/// unusual whitespace/escaping variant the naive pattern doesn't cover).
pub(super) fn replace_json_version(
    content: &str,
    pointer: Option<&str>,
    next: &str,
) -> ReleaseResult<String> {
    let pointer = pointer.unwrap_or("/version");
    let current = read_json_version(content, Some(pointer))?;
    let key = pointer
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .release_context("pointer has no trailing key segment")?;
    replace_single_json_string_field(content, key, &current, next)
}

/// Set both the npm package-lock root `.version` and `.packages[''].version`
/// to `next`, validating the root package name matches first. Same
/// literal-text-substitution approach as [`replace_json_version`], for the
/// same reason (package-lock.json is large and machine-formatted) — but a
/// blind "match every `\"version\": \"OLD\"`" pattern isn't safe here: a
/// dependency can coincidentally pin the exact same version string (e.g.
/// `chai@6.2.2` when axon itself is also 6.2.2). Each of the two real
/// targets is instead anchored to its distinctive neighboring key —
/// `"requires": true` immediately precedes the root version, and
/// `"name": "<package>"` immediately precedes the `packages['']` version —
/// matching the exact structure `read_npm_package_lock_version` already
/// relies on to read them.
pub(super) fn replace_npm_package_lock_version(
    content: &str,
    package: Option<&str>,
    next: &str,
) -> ReleaseResult<String> {
    let package = package.release_context("npm_package_lock requires package")?;
    let current = read_npm_package_lock_version(content, Some(package))?;
    let escaped_current = regex::escape(&current);
    let root = replace_anchored_json_string_field(
        content,
        r#""requires"\s*:\s*true"#,
        &escaped_current,
        next,
        "package-lock root version (anchored after \"requires\": true)",
    )?;
    let both = replace_anchored_json_string_field(
        &root,
        &format!(r#""name"\s*:\s*"{}""#, regex::escape(package)),
        &escaped_current,
        next,
        "package-lock packages[''] version (anchored after \"name\")",
    )?;
    Ok(both)
}

/// Replace the single `"version": "old"` occurrence that appears within a
/// short window after `anchor_pattern` matches. `old_pattern` is a
/// pre-escaped regex fragment (not a literal string) so callers can pass an
/// already-`regex::escape`d value once and reuse it across multiple anchors.
fn replace_anchored_json_string_field(
    content: &str,
    anchor_pattern: &str,
    old_pattern: &str,
    next: &str,
    description: &str,
) -> ReleaseResult<String> {
    let pattern = format!(r#"{anchor_pattern}\s*,?\s*"version"\s*:\s*"{old_pattern}""#);
    let regex = Regex::new(&pattern)
        .with_release_context(|| format!("invalid pattern for {description}"))?;
    let matches = regex.find_iter(content).count();
    if matches != 1 {
        release_bail!("expected exactly 1 occurrence of {description}, found {matches}");
    }
    let mut replaced = false;
    let result = regex.replace(content, |captures: &regex::Captures| {
        replaced = true;
        let whole = captures.get(0).expect("capture 0 always exists").as_str();
        let cut = whole
            .rfind("\"version\"")
            .expect("pattern always contains \"version\"");
        format!("{}\"version\": \"{next}\"", &whole[..cut])
    });
    if !replaced {
        release_bail!("failed to replace {description}");
    }
    Ok(result.into_owned())
}

/// Replace the single literal occurrence of `"key": "old"` (tolerating
/// arbitrary whitespace around the colon) with `"key": "next"`. Errors if
/// that exact substring doesn't appear exactly once.
fn replace_single_json_string_field(
    content: &str,
    key: &str,
    old: &str,
    next: &str,
) -> ReleaseResult<String> {
    let pattern = format!(r#""{}"\s*:\s*"{}""#, regex::escape(key), regex::escape(old));
    let regex = Regex::new(&pattern).release_context("invalid JSON field replacement pattern")?;
    let matches = regex.find_iter(content).count();
    if matches == 0 {
        release_bail!("could not find literal \"{key}\": \"{old}\" in file");
    }
    if matches > 1 {
        release_bail!(
            "\"{key}\": \"{old}\" is ambiguous ({matches} occurrences) — refusing to guess which one to bump"
        );
    }
    Ok(regex
        .replace(content, format!(r#""{key}": "{next}""#))
        .into_owned())
}

/// Insert a `## [next] - DATE` heading (Keep a Changelog style) as the newest
/// entry. Prefers inserting under an existing `## [Unreleased]` heading;
/// otherwise inserts directly above the first `## [` entry (this repo's own
/// convention — its changelogs don't carry an `## [Unreleased]` section).
/// A no-op if the heading is already present.
pub(super) fn replace_changelog_heading(content: &str, next: &str) -> ReleaseResult<String> {
    let heading = format!("## [{next}]");
    if content.lines().any(|line| line.starts_with(&heading)) {
        return Ok(content.to_owned());
    }
    let date = release_date()?;
    let entry = format!("## [{next}] - {date}");

    if let Some(index) = content
        .lines()
        .position(|line| line.starts_with("## [Unreleased]"))
    {
        let mut lines: Vec<String> = content.lines().map(ToOwned::to_owned).collect();
        lines.insert(index + 1, String::new());
        lines.insert(index + 2, entry);
        return Ok(preserve_trailing_newline(content, lines.join("\n")));
    }

    if let Some(index) = content.lines().position(|line| line.starts_with("## [")) {
        let mut lines: Vec<String> = content.lines().map(ToOwned::to_owned).collect();
        lines.insert(index, entry);
        lines.insert(index + 1, String::new());
        return Ok(preserve_trailing_newline(content, lines.join("\n")));
    }

    release_bail!("could not find an insertion point for the changelog heading");
}

fn release_date() -> ReleaseResult<String> {
    let output = std::process::Command::new("date")
        .arg("+%F")
        .output()
        .release_context("failed to run date +%F")?;
    if !output.status.success() {
        release_bail!(
            "date +%F failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let date = String::from_utf8(output.stdout).release_context("date +%F returned non-UTF-8")?;
    let date = date.trim();
    let bytes = date.as_bytes();
    let is_iso_date = bytes.len() == 10
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit);
    if !is_iso_date {
        release_bail!("date +%F returned invalid date: {date}");
    }
    Ok(date.to_owned())
}

fn preserve_trailing_newline(original: &str, mut updated: String) -> String {
    if original.ends_with('\n') && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated
}

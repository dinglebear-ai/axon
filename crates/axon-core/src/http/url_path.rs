//! Small URL path-join helper shared across HTTP probes and clients.

/// Joins `path` onto `base`, normalizing the single slash between them.
///
/// Trailing slashes on `base` and a leading slash on `path` are reconciled so
/// the result always has exactly one separator.
pub fn with_path(base: &str, path: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    if path.starts_with('/') {
        format!("{trimmed}{path}")
    } else {
        format!("{trimmed}/{path}")
    }
}

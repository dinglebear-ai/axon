use std::{fs, path::Path};

pub(super) fn matches_project_filter(
    filter: Option<&str>,
    root: &Path,
    file: &Path,
    relative_key: &str,
) -> bool {
    let Some(raw_filter) = trimmed_filter(filter) else {
        return true;
    };
    let filter = raw_filter.to_ascii_lowercase();

    let path_filter = is_path_filter(raw_filter);
    pathish_contains(relative_key, &filter)
        || (path_filter
            && (pathish_contains(&root.to_string_lossy(), &filter)
                || pathish_contains(&file.to_string_lossy(), &filter)))
        || file_text_contains(file, &filter)
}

fn trimmed_filter(filter: Option<&str>) -> Option<&str> {
    filter.map(str::trim).filter(|value| !value.is_empty())
}

fn is_path_filter(filter: &str) -> bool {
    filter.contains('/') || filter.contains('\\')
}

fn pathish_contains(value: &str, filter: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains(filter) || normalize_separators(&lower).contains(&normalize_separators(filter))
}

fn file_text_contains(file: &Path, filter: &str) -> bool {
    fs::read_to_string(file).is_ok_and(|text| pathish_contains(&text, filter))
}

fn normalize_separators(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | '_' | ' ' => '-',
            _ => ch,
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_filter_allows_all() {
        assert!(matches_project_filter(
            None,
            Path::new("/tmp/root"),
            Path::new("/tmp/root/session.jsonl"),
            "session.jsonl",
        ));
        assert!(matches_project_filter(
            Some("  "),
            Path::new("/tmp/root"),
            Path::new("/tmp/root/session.jsonl"),
            "session.jsonl",
        ));
    }

    #[test]
    fn matches_relative_or_root_path_case_insensitively() {
        assert!(matches_project_filter(
            Some("axon"),
            Path::new("/home/me/.claude/projects"),
            Path::new("/home/me/.claude/projects/-home-me-workspace-Axon/session.jsonl"),
            "-home-me-workspace-Axon/session.jsonl",
        ));
        assert!(matches_project_filter(
            Some("/home/me/workspace/axon"),
            Path::new("/home/me/.claude/projects/-home-me-workspace-axon"),
            Path::new("/home/me/.claude/projects/-home-me-workspace-axon/session.jsonl"),
            "session.jsonl",
        ));
    }

    #[test]
    fn rejects_unmatched_project() {
        assert!(!matches_project_filter(
            Some("other-project"),
            Path::new("/home/me/.codex/sessions"),
            Path::new("/home/me/.codex/sessions/2026/07/15/session.jsonl"),
            "2026/07/15/session.jsonl",
        ));
    }

    #[test]
    fn matches_project_in_file_content() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("rollout.jsonl");
        std::fs::write(
            &file,
            r#"{"type":"session_meta","payload":{"cwd":"/home/me/workspace/axon"}}"#,
        )
        .unwrap();

        assert!(matches_project_filter(
            Some("/home/me/workspace/axon"),
            dir.path(),
            &file,
            "rollout.jsonl",
        ));
    }
}

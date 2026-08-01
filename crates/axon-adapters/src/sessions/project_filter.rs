use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
};

const MAX_METADATA_LINES: usize = 32;
const MAX_METADATA_BYTES: u64 = 64 * 1024;

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
        || file_project_metadata_contains(file, &filter)
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

fn file_project_metadata_contains(file: &Path, filter: &str) -> bool {
    let Ok(file) = File::open(file) else {
        return false;
    };
    let reader = BufReader::new(file.take(MAX_METADATA_BYTES));
    reader
        .lines()
        .take(MAX_METADATA_LINES)
        .filter_map(Result::ok)
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(&line).ok())
        .any(|value| metadata_value_matches(&value, filter))
}

fn metadata_value_matches(value: &serde_json::Value, filter: &str) -> bool {
    const PROJECT_KEYS: [&str; 5] = [
        "cwd",
        "project",
        "project_path",
        "projectPath",
        "projectHash",
    ];
    match value {
        serde_json::Value::Object(object) => object.iter().any(|(key, value)| {
            (PROJECT_KEYS.contains(&key.as_str())
                && value
                    .as_str()
                    .is_some_and(|candidate| pathish_contains(candidate, filter)))
                || metadata_value_matches(value, filter)
        }),
        serde_json::Value::Array(values) => values
            .iter()
            .any(|value| metadata_value_matches(value, filter)),
        _ => false,
    }
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
#[path = "project_filter_tests.rs"]
mod tests;

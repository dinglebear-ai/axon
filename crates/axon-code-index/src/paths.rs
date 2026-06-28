pub(crate) fn code_path_prefixes(relative_path: &str) -> Vec<String> {
    let mut prefixes = Vec::new();
    let mut current = String::new();
    let parts = relative_path.split('/').collect::<Vec<_>>();
    for part in parts.iter().take(parts.len().saturating_sub(1)) {
        if part.is_empty() {
            continue;
        }
        current.push_str(part);
        current.push('/');
        prefixes.push(current.clone());
    }
    prefixes
}

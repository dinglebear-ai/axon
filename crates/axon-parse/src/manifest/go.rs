use crate::facts::inline_text;
use crate::manifest::Dep;
use crate::parser::ParseInput;

pub(super) fn deps(input: &ParseInput) -> Vec<Dep> {
    let mut deps = Vec::new();
    let mut in_require_block = false;
    for (idx, line) in inline_text(input).lines().enumerate() {
        let trimmed = line.trim();
        if trimmed == "require (" {
            in_require_block = true;
            continue;
        }
        if in_require_block && trimmed == ")" {
            in_require_block = false;
            continue;
        }
        let dep_line = trimmed.strip_prefix("require ").unwrap_or(trimmed);
        if dep_line.is_empty()
            || dep_line.starts_with("//")
            || (!in_require_block && dep_line == trimmed)
        {
            continue;
        }
        let mut parts = dep_line.split_whitespace();
        let Some(name) = parts.next() else { continue };
        if name.is_empty() {
            continue;
        }
        deps.push(Dep {
            parser_id: "go_mod",
            ecosystem: "go",
            scope: "require",
            name: name.to_string(),
            version: parts.next().map(ToOwned::to_owned),
            line: idx as u32 + 1,
            quote: trimmed.to_string(),
        });
    }
    deps
}

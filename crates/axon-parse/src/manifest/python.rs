use crate::facts::inline_text;
use crate::manifest::{Dep, quoted_values};
use crate::parser::ParseInput;

pub(super) fn requirements_deps(input: &ParseInput) -> Vec<Dep> {
    inline_text(input)
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            let split_at = split_python_requirement(trimmed);
            let name = trimmed[..split_at].trim();
            if name.is_empty() {
                return None;
            }
            Some(Dep {
                parser_id: "requirements_txt",
                ecosystem: "python",
                scope: "runtime",
                name: name.to_string(),
                version: (split_at < trimmed.len()).then(|| trimmed[split_at..].to_string()),
                line: idx as u32 + 1,
                quote: trimmed.to_string(),
            })
        })
        .collect()
}

pub(super) fn pyproject_deps(input: &ParseInput) -> Vec<Dep> {
    let mut deps = Vec::new();
    for (idx, line) in inline_text(input).lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with("dependencies") {
            continue;
        }
        for quoted in quoted_values(trimmed) {
            let split_at = split_python_requirement(&quoted);
            deps.push(Dep {
                parser_id: "pyproject_toml",
                ecosystem: "python",
                scope: "project.dependencies",
                name: quoted[..split_at].to_string(),
                version: (split_at < quoted.len()).then(|| quoted[split_at..].to_string()),
                line: idx as u32 + 1,
                quote: quoted,
            });
        }
    }
    deps
}

fn split_python_requirement(value: &str) -> usize {
    value
        .find(|ch: char| ['=', '<', '>', '!', '~', '['].contains(&ch))
        .unwrap_or(value.len())
}

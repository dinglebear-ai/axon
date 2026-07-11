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
                fact_kind: "dependency",
                candidate_kind: "manifest_dependency",
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
                fact_kind: "dependency",
                candidate_kind: "manifest_dependency",
                name: quoted[..split_at].to_string(),
                version: (split_at < quoted.len()).then(|| quoted[split_at..].to_string()),
                line: idx as u32 + 1,
                quote: quoted,
            });
        }
    }
    deps
}

/// Extra dependency groups declared under `[project.optional-dependencies]`,
/// e.g. `dev = ["pytest", "black"]`. One fact per extra group (not per
/// package) since the group name is the required "extras" fact per the
/// parsing contract's Python parser family row.
pub(super) fn pyproject_extras(input: &ParseInput) -> Vec<Dep> {
    let mut extras = Vec::new();
    let mut in_extras_section = false;
    for (idx, line) in inline_text(input).lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_extras_section = trimmed == "[project.optional-dependencies]";
            continue;
        }
        if !in_extras_section {
            continue;
        }
        let Some((name, rest)) = trimmed.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let packages = quoted_values(rest);
        extras.push(Dep {
            parser_id: "pyproject_toml",
            ecosystem: "python",
            scope: "project.optional-dependencies",
            fact_kind: "manifest_extra",
            candidate_kind: "manifest_extra",
            name: name.to_string(),
            version: (!packages.is_empty()).then(|| packages.join(", ")),
            line: idx as u32 + 1,
            quote: trimmed.to_string(),
        });
    }
    extras
}

/// `requires-python` in `[project]` pins the interpreter toolchain range —
/// satisfies the parsing contract's Python family toolchain requirement.
pub(super) fn pyproject_toolchain(input: &ParseInput) -> Vec<Dep> {
    let mut toolchain = Vec::new();
    for (idx, line) in inline_text(input).lines().enumerate() {
        let trimmed = line.trim();
        let Some((key, rest)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() != "requires-python" {
            continue;
        }
        for quoted in quoted_values(rest) {
            toolchain.push(Dep {
                parser_id: "pyproject_toml",
                ecosystem: "python",
                scope: "project",
                fact_kind: "toolchain_version",
                candidate_kind: "toolchain_version",
                name: "python".to_string(),
                version: Some(quoted),
                line: idx as u32 + 1,
                quote: trimmed.to_string(),
            });
        }
    }
    toolchain
}

fn split_python_requirement(value: &str) -> usize {
    value
        .find(|ch: char| ['=', '<', '>', '!', '~', '['].contains(&ch))
        .unwrap_or(value.len())
}

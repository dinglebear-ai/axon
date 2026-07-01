use crate::facts::inline_text;
use crate::manifest::{Dep, first_quoted};
use crate::parser::ParseInput;

pub(super) fn deps(input: &ParseInput) -> Vec<Dep> {
    let mut deps = Vec::new();
    let mut scope: Option<&'static str> = None;
    for (idx, line) in inline_text(input).lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            scope = match trimmed {
                "[dependencies]" => Some("dependencies"),
                "[dev-dependencies]" => Some("dev-dependencies"),
                "[build-dependencies]" => Some("build-dependencies"),
                _ => None,
            };
            continue;
        }
        let Some(scope) = scope else { continue };
        let Some((name, rest)) = trimmed.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || name.starts_with('#') {
            continue;
        }
        deps.push(Dep {
            parser_id: "cargo_manifest",
            ecosystem: "cargo",
            scope,
            name: name.to_string(),
            version: first_quoted(rest),
            line: idx as u32 + 1,
            quote: trimmed.to_string(),
        });
    }
    deps
}

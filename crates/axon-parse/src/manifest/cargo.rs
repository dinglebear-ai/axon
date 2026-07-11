use crate::facts::inline_text;
use crate::manifest::{Dep, first_quoted, quoted_values};
use crate::parser::ParseInput;

pub(super) fn deps(input: &ParseInput) -> Vec<Dep> {
    let mut deps = Vec::new();
    let mut scope: Option<&'static str> = None;
    for (idx, line) in inline_text(input).lines().enumerate() {
        let trimmed = line.trim();
        let line_no = idx as u32 + 1;
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            scope = match trimmed {
                "[dependencies]" => Some("dependencies"),
                "[dev-dependencies]" => Some("dev-dependencies"),
                "[build-dependencies]" => Some("build-dependencies"),
                "[workspace]" => Some("workspace"),
                "[features]" => Some("features"),
                "[package]" => Some("package"),
                _ => None,
            };
            continue;
        }
        let Some(scope) = scope else { continue };
        let Some((key, rest)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || key.starts_with('#') {
            continue;
        }
        push_entry(&mut deps, scope, key, rest, trimmed, line_no);
    }
    deps
}

fn push_entry(
    deps: &mut Vec<Dep>,
    scope: &'static str,
    key: &str,
    rest: &str,
    trimmed: &str,
    line_no: u32,
) {
    match scope {
        "workspace" if key == "members" => {
            for member in quoted_values(rest) {
                deps.push(Dep {
                    parser_id: "cargo_manifest",
                    ecosystem: "cargo",
                    scope: "workspace.members",
                    fact_kind: "workspace_member",
                    candidate_kind: "workspace_member",
                    name: member,
                    version: None,
                    line: line_no,
                    quote: trimmed.to_string(),
                });
            }
        }
        "features" => {
            deps.push(Dep {
                parser_id: "cargo_manifest",
                ecosystem: "cargo",
                scope: "features",
                fact_kind: "manifest_feature",
                candidate_kind: "manifest_feature",
                name: key.to_string(),
                version: None,
                line: line_no,
                quote: trimmed.to_string(),
            });
        }
        "dependencies" | "dev-dependencies" | "build-dependencies" => {
            deps.push(Dep {
                parser_id: "cargo_manifest",
                ecosystem: "cargo",
                scope,
                fact_kind: "dependency",
                candidate_kind: "manifest_dependency",
                name: key.to_string(),
                version: first_quoted(rest),
                line: line_no,
                quote: trimmed.to_string(),
            });
        }
        // Rust toolchain facts: `edition` names the language toolchain,
        // `rust-version` pins the MSRV — satisfies the parsing contract's
        // Rust family "toolchain" fact requirement.
        "package" if key == "edition" => {
            if let Some(edition) = first_quoted(rest) {
                deps.push(Dep {
                    parser_id: "cargo_manifest",
                    ecosystem: "cargo",
                    scope: "package",
                    fact_kind: "toolchain",
                    candidate_kind: "toolchain",
                    name: "rust".to_string(),
                    version: Some(edition),
                    line: line_no,
                    quote: trimmed.to_string(),
                });
            }
        }
        "package" if key == "rust-version" => {
            if let Some(version) = first_quoted(rest) {
                deps.push(Dep {
                    parser_id: "cargo_manifest",
                    ecosystem: "cargo",
                    scope: "package",
                    fact_kind: "toolchain_version",
                    candidate_kind: "toolchain_version",
                    name: "rust".to_string(),
                    version: Some(version),
                    line: line_no,
                    quote: trimmed.to_string(),
                });
            }
        }
        _ => {}
    }
}

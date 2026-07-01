use axon_api::source::{ContentKind, GraphCandidate, SourceParseFacts};
use serde_json::{Value, json};

use crate::facts::{inline_text, source_fact};
use crate::graph_candidate::graph_candidate;
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "manifest";

pub fn dependency_facts(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let path = input.document.path.as_deref().unwrap_or_default();
    let deps = if path.ends_with("Cargo.toml") {
        cargo_deps(input)
    } else if path.ends_with("package.json") {
        package_json_deps(input)
    } else if path.ends_with("requirements.txt") {
        requirements_deps(input)
    } else if path.ends_with("pyproject.toml") {
        pyproject_deps(input)
    } else if path.ends_with("go.mod") {
        go_mod_deps(input)
    } else if path.ends_with("pom.xml") {
        maven_pom_deps(input)
    } else {
        Vec::new()
    };

    let mut facts = Vec::new();
    let mut candidates = Vec::new();
    for dep in deps {
        facts.push(source_fact(
            input,
            dep.parser_id,
            "compact_manifest",
            "dependency",
            dep.name.clone(),
            json!({
                "ecosystem": dep.ecosystem,
                "version": dep.version,
                "scope": dep.scope,
            }),
            Some(dep.line),
        ));
        candidates.push(graph_candidate(
            input,
            dep.parser_id,
            "manifest_dependency",
            &dep.name,
            Some(dep.line),
            Some(dep.quote),
        ));
    }
    for resource in yaml_iac_resources(input) {
        facts.push(source_fact(
            input,
            "yaml_iac_manifest",
            "yaml_iac_heuristic",
            "iac_resource",
            resource.name.clone(),
            json!({
                "api_version": resource.api_version,
                "kind": resource.kind,
                "resource_name": resource.resource_name,
            }),
            Some(resource.line),
        ));
        candidates.push(graph_candidate(
            input,
            "yaml_iac_manifest",
            "iac_resource",
            &resource.name,
            Some(resource.line),
            Some(resource.quote),
        ));
    }
    (facts, candidates)
}

struct Dep {
    parser_id: &'static str,
    ecosystem: &'static str,
    scope: &'static str,
    name: String,
    version: Option<String>,
    line: u32,
    quote: String,
}

struct IacResource {
    name: String,
    api_version: String,
    kind: String,
    resource_name: String,
    line: u32,
    quote: String,
}

fn cargo_deps(input: &ParseInput) -> Vec<Dep> {
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

fn package_json_deps(input: &ParseInput) -> Vec<Dep> {
    let Ok(root) = serde_json::from_str::<Value>(inline_text(input)) else {
        return Vec::new();
    };
    let mut deps = Vec::new();
    for scope in ["dependencies", "devDependencies", "peerDependencies"] {
        let Some(obj) = root.get(scope).and_then(Value::as_object) else {
            continue;
        };
        for (name, version) in obj {
            deps.push(Dep {
                parser_id: "package_json",
                ecosystem: "npm",
                scope,
                name: name.clone(),
                version: version.as_str().map(ToOwned::to_owned),
                line: 1,
                quote: format!("{name}: {version}"),
            });
        }
    }
    deps
}

fn requirements_deps(input: &ParseInput) -> Vec<Dep> {
    inline_text(input)
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            let split_at = trimmed
                .find(|ch: char| ['=', '<', '>', '!', '~', '['].contains(&ch))
                .unwrap_or(trimmed.len());
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

fn pyproject_deps(input: &ParseInput) -> Vec<Dep> {
    let mut deps = Vec::new();
    for (idx, line) in inline_text(input).lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with("dependencies") {
            continue;
        }
        for quoted in quoted_values(trimmed) {
            let split_at = quoted
                .find(|ch: char| ['=', '<', '>', '!', '~', '['].contains(&ch))
                .unwrap_or(quoted.len());
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

fn go_mod_deps(input: &ParseInput) -> Vec<Dep> {
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

fn maven_pom_deps(input: &ParseInput) -> Vec<Dep> {
    let text = inline_text(input);
    dependency_blocks(inline_text(input))
        .into_iter()
        .filter_map(|block| {
            let group = tag_value(block, "groupId")?;
            let artifact = tag_value(block, "artifactId")?;
            let offset = block.as_ptr() as usize - text.as_ptr() as usize;
            Some(Dep {
                parser_id: "maven_pom",
                ecosystem: "maven",
                scope: "dependencies",
                name: format!("{group}:{artifact}"),
                version: tag_value(block, "version").map(ToOwned::to_owned),
                line: line_for_offset(text, offset),
                quote: compact_quote(block),
            })
        })
        .collect()
}

fn yaml_iac_resources(input: &ParseInput) -> Vec<IacResource> {
    let path = input.document.path.as_deref().unwrap_or_default();
    if input.document.content_kind != ContentKind::Yaml
        && !path.ends_with(".yaml")
        && !path.ends_with(".yml")
    {
        return Vec::new();
    }

    let mut resources = Vec::new();
    let mut doc_start_line = 1;
    let mut doc_lines = Vec::new();
    for (idx, line) in inline_text(input).lines().enumerate() {
        if line.trim() == "---" {
            push_yaml_resource(&mut resources, doc_start_line, &doc_lines);
            doc_start_line = idx as u32 + 2;
            doc_lines.clear();
        } else {
            doc_lines.push(line);
        }
    }
    push_yaml_resource(&mut resources, doc_start_line, &doc_lines);
    resources
}

fn push_yaml_resource(resources: &mut Vec<IacResource>, start_line: u32, lines: &[&str]) {
    let mut api_version: Option<String> = None;
    let mut kind: Option<String> = None;
    let mut metadata_indent: Option<usize> = None;
    let mut metadata_name: Option<String> = None;
    let mut top_level_name: Option<String> = None;
    let mut kind_line = start_line;

    for (offset, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();
        if let Some(value) = yaml_scalar(trimmed, "apiVersion") {
            api_version = Some(value.to_string());
        } else if let Some(value) = yaml_scalar(trimmed, "kind") {
            kind = Some(value.to_string());
            kind_line = start_line + offset as u32;
        } else if trimmed == "metadata:" {
            metadata_indent = Some(indent);
        } else if let Some(value) = yaml_scalar(trimmed, "name") {
            if metadata_indent.is_some_and(|metadata| indent > metadata) {
                metadata_name = Some(value.to_string());
            } else if indent == 0 {
                top_level_name = Some(value.to_string());
            }
        }
    }

    let Some(api_version) = api_version else {
        return;
    };
    let Some(resource_name) = metadata_name.or(top_level_name) else {
        return;
    };
    let kind = kind.unwrap_or_else(|| {
        if api_version == "v2" {
            "Chart".to_string()
        } else {
            "YamlResource".to_string()
        }
    });
    resources.push(IacResource {
        name: format!("{kind}/{resource_name}"),
        api_version,
        kind: kind.clone(),
        resource_name,
        line: kind_line,
        quote: format!("kind: {kind}"),
    });
}

fn first_quoted(value: &str) -> Option<String> {
    quoted_values(value).into_iter().next()
}

fn quoted_values(value: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut chars = value.char_indices();
    while let Some((start, ch)) = chars.next() {
        if ch != '"' && ch != '\'' {
            continue;
        }
        let quote = ch;
        if let Some((end, _)) = chars.by_ref().find(|(_, candidate)| *candidate == quote) {
            values.push(value[start + ch.len_utf8()..end].to_string());
        }
    }
    values
}

fn dependency_blocks(text: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("<dependency>") {
        rest = &rest[start..];
        let Some(end) = rest.find("</dependency>") else {
            break;
        };
        let block_end = end + "</dependency>".len();
        blocks.push(&rest[..block_end]);
        rest = &rest[block_end..];
    }
    blocks
}

fn tag_value<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let value = text.split(&open).nth(1)?.split(&close).next()?.trim();
    (!value.is_empty()).then_some(value)
}

fn line_for_offset(text: &str, offset: usize) -> u32 {
    text[..offset.min(text.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count() as u32
        + 1
}

fn compact_quote(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn yaml_scalar<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let value = line.strip_prefix(key)?.strip_prefix(':')?.trim();
    let value = value.trim_matches('"').trim_matches('\'');
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;

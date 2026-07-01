use axon_api::source::ContentKind;

use crate::facts::inline_text;
use crate::manifest::{IacResource, yaml_scalar};
use crate::parser::ParseInput;

pub(super) fn resources(input: &ParseInput) -> Vec<IacResource> {
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
            push_resource(&mut resources, doc_start_line, &doc_lines);
            doc_start_line = idx as u32 + 2;
            doc_lines.clear();
        } else {
            doc_lines.push(line);
        }
    }
    push_resource(&mut resources, doc_start_line, &doc_lines);
    resources
}

fn push_resource(resources: &mut Vec<IacResource>, start_line: u32, lines: &[&str]) {
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

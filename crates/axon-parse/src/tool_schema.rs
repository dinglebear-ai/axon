//! `ToolSchemaParser` — CLI help text and MCP tool schema documents.
//!
//! Distinct from `tool.rs` (`ToolParser`/`tool_output_jsonl`), which parses
//! JSONL *tool call/output* records. This module parses the *definition*
//! surface described by the parsing contract's CLI/MCP Tool family row:
//! tools, arguments, return shapes, and side-effect class, plus an
//! `external_resource` graph node per tool representing the external
//! capability the tool exposes.

use axon_api::source::{GraphCandidate, SourceParseFacts};
use serde_json::{Value, json};

use crate::facts::{inline_text, source_fact};
use crate::graph_candidate::{candidate_edge, graph_candidate};
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "tool_schema";

const MAX_TOOL_SCHEMA_ENTRIES: usize = 512;
const MAX_TOOL_SCHEMA_ARGS: usize = 256;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolSchemaParseItems {
    pub facts: Vec<SourceParseFacts>,
    pub graph_candidates: Vec<GraphCandidate>,
}

pub fn tool_schema_parse_items(input: &ParseInput) -> ToolSchemaParseItems {
    if let Some(payload) = input.document.structured_payload.as_ref() {
        return mcp_schema_items(input, payload);
    }
    let text = inline_text(input);
    if let Ok(value) = serde_json::from_str::<Value>(text.trim()) {
        return mcp_schema_items(input, &value);
    }
    cli_help_items(input, text)
}

fn mcp_schema_items(input: &ParseInput, value: &Value) -> ToolSchemaParseItems {
    let mut parsed = ToolSchemaParseItems::default();
    for tool in extract_tool_entries(value)
        .into_iter()
        .take(MAX_TOOL_SCHEMA_ENTRIES)
    {
        push_mcp_tool(input, tool, &mut parsed);
    }
    parsed
}

fn extract_tool_entries(value: &Value) -> Vec<&Value> {
    if let Some(tools) = value.get("tools").and_then(Value::as_array) {
        return tools.iter().collect();
    }
    if let Some(array) = value.as_array() {
        return array.iter().collect();
    }
    if value.get("name").is_some() {
        return vec![value];
    }
    Vec::new()
}

fn push_mcp_tool(input: &ParseInput, tool: &Value, parsed: &mut ToolSchemaParseItems) {
    let Some(name) = tool.get("name").and_then(Value::as_str) else {
        return;
    };
    let description = tool
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let side_effect_class = infer_side_effect_class(name, description);
    let input_schema = tool.get("inputSchema").or_else(|| tool.get("input_schema"));
    let arg_names = schema_property_names(input_schema);
    let required = schema_required(input_schema);
    let has_output_schema = tool
        .get("outputSchema")
        .or_else(|| tool.get("output_schema"))
        .is_some();
    let output_kind = if has_output_schema {
        "structured"
    } else {
        "unspecified"
    };

    push_tool_fact(
        input,
        "tool_schema_mcp",
        "mcp_schema",
        &mut parsed.facts,
        "tool_definition",
        name.to_string(),
        json!({
            "description": description,
            "argument_count": arg_names.len(),
            "output_kind": output_kind,
            "side_effect_class": side_effect_class,
            "source": "mcp_schema",
        }),
    );

    for arg in arg_names.iter().take(MAX_TOOL_SCHEMA_ARGS) {
        push_tool_fact(
            input,
            "tool_schema_mcp",
            "mcp_schema",
            &mut parsed.facts,
            "tool_argument",
            format!("{name}.{arg}"),
            json!({ "tool": name, "argument": arg, "required": required.contains(arg) }),
        );
    }

    if has_output_schema {
        push_tool_fact(
            input,
            "tool_schema_mcp",
            "mcp_schema",
            &mut parsed.facts,
            "tool_return_shape",
            format!("{name}.output"),
            json!({ "tool": name, "output_kind": output_kind }),
        );
    }

    push_tool_graph_candidates(
        input,
        "tool_schema_mcp",
        "text_mention",
        &mut parsed.graph_candidates,
        name,
        description,
    );
}

fn push_tool_fact(
    input: &ParseInput,
    parser_id: &str,
    parser_method: &str,
    facts: &mut Vec<SourceParseFacts>,
    fact_kind: &str,
    name: String,
    value: Value,
) {
    facts.push(source_fact(
        input,
        parser_id,
        parser_method,
        fact_kind,
        name,
        value,
        None,
    ));
}

/// Emits the tool's own `tool_definition` graph node plus a synthetic
/// `tool_call` discovery-event node linking to the reusable `tool` node
/// (`tool_call_uses_tool`) and an `external_resource` node
/// (`tool_call_read_resource`) representing the external capability the
/// parsed document exposes — satisfying the contract's "external-resource
/// graph nodes" requirement for this parser family using only edge/node
/// kinds from axon-graph's closed `GraphEdgeKind`/`GraphNodeKind` registries
/// (`docs/pipeline-unification/sources/source-graph.md`: standalone
/// `cli_tool`/`mcp_tool` source jobs create the reusable `tool` node, a
/// `tool_call` node, and any `external_resource` nodes with provenance
/// edges — there is no direct tool -> external_resource edge kind in the
/// registry).
fn push_tool_graph_candidates(
    input: &ParseInput,
    parser_id: &str,
    evidence_kind: &str,
    candidates: &mut Vec<GraphCandidate>,
    name: &str,
    description: &str,
) {
    candidates.push(graph_candidate(
        input,
        parser_id,
        "tool_definition",
        name,
        None,
        Some(description.to_string()),
    ));

    let tool_call_key = format!("tool-call:{parser_id}:{name}");
    candidates.push(candidate_edge(
        input,
        parser_id,
        "tool_schema_uses_tool",
        "tool_call",
        &tool_call_key,
        "tool",
        &format!("tool:{name}"),
        "tool_call_uses_tool",
        evidence_kind,
        None,
        Some(description.to_string()),
    ));
    candidates.push(candidate_edge(
        input,
        parser_id,
        "tool_schema_reads_resource",
        "tool_call",
        &tool_call_key,
        "external_resource",
        &format!("external:tool:{name}"),
        "tool_call_read_resource",
        evidence_kind,
        None,
        Some(description.to_string()),
    ));
}

fn schema_property_names(schema: Option<&Value>) -> Vec<String> {
    schema
        .and_then(|schema| schema.get("properties"))
        .and_then(Value::as_object)
        .map(|properties| properties.keys().cloned().collect())
        .unwrap_or_default()
}

fn schema_required(schema: Option<&Value>) -> Vec<String> {
    schema
        .and_then(|schema| schema.get("required"))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn infer_side_effect_class(name: &str, description: &str) -> &'static str {
    let haystack = format!("{name} {description}").to_ascii_lowercase();
    if ["delete", "remove", "destroy", "purge"]
        .iter()
        .any(|needle| haystack.contains(needle))
    {
        "delete"
    } else if [
        "write", "create", "update", "set", "upsert", "push", "publish",
    ]
    .iter()
    .any(|needle| haystack.contains(needle))
    {
        "write"
    } else if ["send", "notify", "call", "execute", "run"]
        .iter()
        .any(|needle| haystack.contains(needle))
    {
        "mutate"
    } else {
        "read"
    }
}

fn cli_help_items(input: &ParseInput, text: &str) -> ToolSchemaParseItems {
    let mut parsed = ToolSchemaParseItems::default();
    let Some(command) = usage_command(text) else {
        return parsed;
    };
    push_cli_tool(input, &command, text, &mut parsed);
    for (name, description) in section_entries(text, "Commands:") {
        push_cli_subcommand(input, &command, &name, &description, &mut parsed);
    }
    for (flag, description) in section_entries(text, "Options:") {
        push_cli_argument(input, &command, &flag, &description, &mut parsed);
    }
    parsed
}

fn usage_command(text: &str) -> Option<String> {
    let line = text.lines().find(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("Usage:") || trimmed.starts_with("USAGE:")
    })?;
    let rest = line.trim_start().split_once(':')?.1.trim();
    rest.split_whitespace().next().map(str::to_string)
}

fn push_cli_tool(input: &ParseInput, command: &str, text: &str, parsed: &mut ToolSchemaParseItems) {
    let description = text
        .lines()
        .find(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with("Usage") && !trimmed.starts_with("USAGE")
        })
        .unwrap_or_default()
        .trim();
    push_tool_fact(
        input,
        "tool_schema_cli",
        "cli_help_heuristic",
        &mut parsed.facts,
        "tool_definition",
        command.to_string(),
        json!({ "description": description, "source": "cli_help" }),
    );
    push_tool_graph_candidates(
        input,
        "tool_schema_cli",
        "text_mention",
        &mut parsed.graph_candidates,
        command,
        description,
    );
}

/// Subcommands are modeled as their own `tool` definitions (with the
/// `parent_tool` relationship captured in the fact payload, not as a graph
/// edge): the closed `GraphEdgeKind` registry has no tool -> tool hierarchy
/// edge kind, so this reuses the same registry-valid `tool_call_uses_tool` /
/// `tool_call_read_resource` shape as `push_tool_graph_candidates` rather
/// than inventing an edge kind the graph store would reject.
fn push_cli_subcommand(
    input: &ParseInput,
    command: &str,
    name: &str,
    description: &str,
    parsed: &mut ToolSchemaParseItems,
) {
    let side_effect_class = infer_side_effect_class(name, description);
    let full_name = format!("{command} {name}");
    push_tool_fact(
        input,
        "tool_schema_cli",
        "cli_help_heuristic",
        &mut parsed.facts,
        "tool_definition",
        full_name.clone(),
        json!({
            "description": description,
            "side_effect_class": side_effect_class,
            "source": "cli_help",
            "parent_tool": command,
        }),
    );
    push_tool_graph_candidates(
        input,
        "tool_schema_cli",
        "text_mention",
        &mut parsed.graph_candidates,
        &full_name,
        description,
    );
}

fn push_cli_argument(
    input: &ParseInput,
    command: &str,
    flag: &str,
    description: &str,
    parsed: &mut ToolSchemaParseItems,
) {
    push_tool_fact(
        input,
        "tool_schema_cli",
        "cli_help_heuristic",
        &mut parsed.facts,
        "tool_argument",
        format!("{command}.{flag}"),
        json!({ "tool": command, "argument": flag, "description": description }),
    );
}

/// Scans the indented block following a `header` line (e.g. `"Commands:"`)
/// for `name<2+ spaces>description` rows, stopping at the first blank line
/// or line that returns to column 0.
fn section_entries(text: &str, header: &str) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    let mut in_section = false;
    for line in text.lines() {
        if !in_section {
            if line.trim() == header {
                in_section = true;
            }
            continue;
        }
        if line.trim().is_empty() {
            break;
        }
        if !line.starts_with(char::is_whitespace) {
            break;
        }
        let content = line.trim();
        let Some(split_at) = column_split(content) else {
            continue;
        };
        let name = content[..split_at].trim();
        let description = content[split_at..].trim();
        if name.is_empty() {
            continue;
        }
        entries.push((name.to_string(), description.to_string()));
        if entries.len() >= MAX_TOOL_SCHEMA_ENTRIES {
            break;
        }
    }
    entries
}

/// Finds the first run of 2+ ASCII spaces, the conventional column separator
/// in `--help` output between a name/flag and its description.
fn column_split(content: &str) -> Option<usize> {
    let bytes = content.as_bytes();
    (0..bytes.len().saturating_sub(1)).find(|&idx| bytes[idx] == b' ' && bytes[idx + 1] == b' ')
}

#[cfg(test)]
#[path = "tool_schema_tests.rs"]
mod tests;

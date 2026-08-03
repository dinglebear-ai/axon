use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use sha2::{Digest, Sha256};

use super::artifact::{DocsArtifactSet, GeneratedDocArtifact};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
#[value(rename_all = "kebab-case")]
/// `adapter-scopes.{json,md}` are intentionally absent: the schemas generator
/// owns both (`xtask/src/schemas/adapters.rs`). Declaring them here too made
/// `schemas generate --check` and `docs generate --check` mutually
/// unsatisfiable once adapter data changed. One owner per artifact path.
pub enum DocsFamily {
    Cli,
    CliHelp,
    Openapi,
    Mcp,
    ApiDto,
    ApiEnums,
    Errors,
    Events,
    Config,
    Env,
    Providers,
    Schema,
    Memory,
    Presentation,
    Schemas,
    NewSource,
}

impl DocsFamily {
    pub const ALL: [Self; 16] = [
        Self::Cli,
        Self::CliHelp,
        Self::Openapi,
        Self::Mcp,
        Self::ApiDto,
        Self::ApiEnums,
        Self::Errors,
        Self::Events,
        Self::Config,
        Self::Env,
        Self::Schema,
        Self::Memory,
        Self::Providers,
        Self::Presentation,
        Self::Schemas,
        Self::NewSource,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::CliHelp => "cli-help",
            Self::Openapi => "openapi",
            Self::Mcp => "mcp",
            Self::ApiDto => "api-dto",
            Self::ApiEnums => "api-enums",
            Self::Errors => "errors",
            Self::Events => "events",
            Self::Config => "config",
            Self::Env => "env",
            Self::Providers => "providers",
            Self::Schema => "schema",
            Self::Memory => "memory",
            Self::Presentation => "presentation",
            Self::Schemas => "schemas",
            Self::NewSource => "new-source",
        }
    }

    const fn input_path(self) -> &'static str {
        match self {
            Self::Cli | Self::CliHelp => "docs/reference/cli/commands.json",
            Self::Openapi => "docs/reference/rest/openapi.json",
            Self::Mcp => "docs/reference/mcp/tool-schema.json",
            Self::ApiDto | Self::ApiEnums => "docs/reference/api/schemas.json",
            Self::Errors => "docs/reference/api/errors.schema.json",
            Self::Events => "docs/reference/runtime/events.schema.json",
            Self::Config => "docs/reference/config/config.schema.json",
            Self::Env => "docs/reference/config/env.schema.json",
            Self::Providers => "docs/reference/runtime/provider-capabilities.schema.json",
            Self::Schema => "docs/reference/runtime/database-schema.json",
            Self::Memory => "docs/reference/runtime/database-schema.json",
            Self::Presentation => "docs/reference/presentation/tokens.schema.json",
            Self::Schemas => "docs/reference/api/schemas.json",
            Self::NewSource => "docs/reference/sources/adapter-scopes.json",
        }
    }

    const fn output_path(self) -> &'static str {
        match self {
            Self::Cli => "docs/reference/generated/cli.md",
            Self::CliHelp => "docs/reference/generated/cli-help.md",
            Self::Openapi => "docs/reference/generated/openapi.md",
            Self::Mcp => "docs/reference/generated/mcp.md",
            Self::ApiDto => "docs/reference/api/dto.md",
            Self::ApiEnums => "docs/reference/api/enums.md",
            Self::Errors => "docs/reference/generated/errors.md",
            Self::Events => "docs/reference/runtime/observability.md",
            Self::Config => "docs/reference/generated/config.md",
            Self::Env => "docs/reference/generated/env.md",
            Self::Providers => "docs/reference/runtime/providers.md",
            Self::Schema => "docs/reference/runtime/schema.md",
            Self::Memory => "docs/reference/generated/memory.md",
            Self::Presentation => "docs/reference/generated/presentation.md",
            Self::Schemas => "docs/reference/generated/schemas.md",
            Self::NewSource => "docs/reference/generated/new-source.md",
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Cli => "CLI Command Reference",
            Self::CliHelp => "CLI Help Reference",
            Self::Openapi => "OpenAPI Reference",
            Self::Mcp => "MCP Tool Reference",
            Self::ApiDto => "API DTO Reference",
            Self::ApiEnums => "API Enum Reference",
            Self::Errors => "API Error Reference",
            Self::Events => "Runtime Event Reference",
            Self::Config => "Configuration Reference",
            Self::Env => "Environment Reference",
            Self::Providers => "Provider Capability Reference",
            Self::Schema => "Runtime Database Schema",
            Self::Memory => "Memory Reference",
            Self::Presentation => "Presentation Reference",
            Self::Schemas => "Schema Reference",
            Self::NewSource => "New Source Reference",
        }
    }
}

#[cfg(test)]
pub(crate) fn generated_output_paths() -> impl Iterator<Item = &'static str> {
    DocsFamily::ALL.into_iter().map(DocsFamily::output_path)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInput {
    pub path: PathBuf,
    pub checksum: String,
}

pub trait DocsFamilyGenerator {
    fn family(&self) -> DocsFamily;
    fn source_inputs(&self, root: &Path) -> Result<Vec<SourceInput>>;
    fn generate(&self, root: &Path) -> Result<DocsArtifactSet>;
}

pub struct JsonFamilyGenerator(pub DocsFamily);

impl DocsFamilyGenerator for JsonFamilyGenerator {
    fn family(&self) -> DocsFamily {
        self.0
    }

    fn source_inputs(&self, root: &Path) -> Result<Vec<SourceInput>> {
        let relative = PathBuf::from(self.0.input_path());
        let content = std::fs::read(root.join(&relative)).with_context(|| {
            format!(
                "docs {} source input is missing: {}",
                self.0.slug(),
                relative.display()
            )
        })?;
        Ok(vec![SourceInput {
            path: relative,
            checksum: format!("sha256:{:x}", Sha256::digest(&content)),
        }])
    }

    fn generate(&self, root: &Path) -> Result<DocsArtifactSet> {
        let source_inputs = self.source_inputs(root)?;
        let input = &source_inputs[0];
        let raw = std::fs::read_to_string(root.join(&input.path))?;
        let json: serde_json::Value = serde_json::from_str(&raw)
            .with_context(|| format!("docs {} source input is not valid JSON", self.0.slug()))?;
        let rendered = render_json_reference(self.0, input, &json)?;
        let artifact = GeneratedDocArtifact::new(self.0.output_path(), rendered, self.0.slug())?;
        Ok(DocsArtifactSet {
            family: self.0,
            artifacts: vec![artifact],
            source_inputs,
        })
    }
}

fn render_json_reference(
    family: DocsFamily,
    input: &SourceInput,
    value: &serde_json::Value,
) -> Result<String> {
    let mut content = format!(
        "<!-- generated by cargo xtask docs {}; do not edit directly -->\n<!-- source inputs: {} -->\n\n# {}\n\nGenerated from [`{}`](../../../{}). The linked JSON artifact is the complete machine-readable contract; this page is its deterministic human index.\n\n## Source Manifest\n\n| Path | SHA-256 |\n|---|---|\n| `{}` | `{}` |\n\n## Top-Level Inventory\n\n{}\n\n## Named Definitions\n\n{}\n",
        family.slug(),
        input.checksum,
        family.title(),
        input.path.display(),
        input.path.display(),
        input.path.display(),
        input.checksum,
        top_level_inventory(value),
        named_definitions(value),
    );
    content.truncate(content.trim_end().len());
    content.push('\n');
    if content.lines().count() < 8 {
        bail!(
            "docs {} renderer produced incomplete content",
            family.slug()
        );
    }
    Ok(content)
}

fn top_level_inventory(value: &serde_json::Value) -> String {
    let Some(object) = value.as_object() else {
        return "The source artifact is not a JSON object.\n".to_owned();
    };
    let mut rows = object
        .iter()
        .map(|(key, value)| format!("| `{key}` | {} |", value_shape(value)))
        .collect::<Vec<_>>();
    rows.sort();
    format!("| Field | Shape |\n|---|---|\n{}", rows.join("\n"))
}

fn named_definitions(value: &serde_json::Value) -> String {
    let Some(object) = value.as_object() else {
        return "No named definitions.\n".to_owned();
    };
    let mut names = ["definitions", "$defs", "tables", "properties", "components"]
        .into_iter()
        .flat_map(|key| named_members(object.get(key)))
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    if names.is_empty() {
        "No named definitions in this artifact.\n".to_owned()
    } else {
        names
            .into_iter()
            .map(|name| format!("- `{name}`"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }
}

fn named_members(value: Option<&serde_json::Value>) -> Vec<String> {
    match value {
        Some(serde_json::Value::Object(values)) => values.keys().cloned().collect(),
        Some(serde_json::Value::Array(values)) => values
            .iter()
            .filter_map(|value| {
                value
                    .get("name")
                    .or_else(|| value.get("table"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn value_shape(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Array(values) => format!("array ({})", values.len()),
        serde_json::Value::Object(values) => format!("object ({})", values.len()),
        serde_json::Value::String(_) => "string".to_owned(),
        serde_json::Value::Number(_) => "number".to_owned(),
        serde_json::Value::Bool(_) => "boolean".to_owned(),
        serde_json::Value::Null => "null".to_owned(),
    }
}

pub fn generator_for(family: DocsFamily) -> Box<dyn DocsFamilyGenerator> {
    Box::new(JsonFamilyGenerator(family))
}

#[cfg(test)]
#[path = "families_tests.rs"]
mod tests;

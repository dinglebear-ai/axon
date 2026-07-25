use super::CANONICAL_ENUMS;
use super::api_defs;
use crate::schemas::source_input::SourceInput;
use serde_json::Value;

pub(super) fn enum_markdown() -> String {
    let mut out = generated_header("api-enums", "api");
    out.push_str("## Source Inputs\n\nSee `dto.md` and `schemas.json` for the API schema source-input manifest.\n\n");
    out.push_str(
        "## Root Shape\n\nEnum registry projection generated with the API schema family.\n\n",
    );
    out.push_str("## Required Definitions\n\nSee `docs/reference/api/schemas.json`.\n\n");
    out.push_str("## Field Tables\n\nNot applicable to enum-only projection.\n\n");
    out.push_str("## Enum Tables\n\n");
    out.push_str("| Enum | Values |\n|---|---|\n");
    for (name, values) in CANONICAL_ENUMS {
        out.push_str(&format!("| `{name}` | `{}` |\n", values.join("`, `")));
    }
    out.push_str("\n## Extension Points\n\nEnum extensions require contract updates.\n\n");
    out.push_str("## Forbidden Fields\n\nNot applicable to enum-only projection.\n\n");
    out.push_str("## Examples\n\nExamples validate through the API schema fixture set.\n\n");
    out.push_str("## Fixture Paths\n\n`xtask/tests/fixtures/schemas/api`.\n\n");
    out.push_str("## Drift Checks\n\nRun `cargo xtask schemas api --check`.\n");
    out
}

pub(super) fn markdown(family: &str, inputs: &[SourceInput]) -> String {
    let mut out = generated_header(family, family);
    out.push_str("## Source Inputs\n\n| Path | SHA-256 |\n|---|---|\n");
    for input in inputs {
        out.push_str(&format!("| `{}` | `{}` |\n", input.path, input.checksum));
    }
    out.push_str("\n## Root Shape\n\nGenerated JSON schema object.\n\n");
    out.push_str("## Required Definitions\n\nSee the generated JSON artifact.\n\n");
    out.push_str("## Field Tables\n\nGenerated from the same registry model as JSON.\n\n");
    out.push_str(
        "## Enum Tables\n\nGenerated from registry enum projections where applicable.\n\n",
    );
    out.push_str("## Extension Points\n\nExtension points are declared in the source registry when allowed.\n\n");
    out.push_str(
        "## Forbidden Fields\n\nRemoved and secret fields are rejected by schema checks.\n\n",
    );
    out.push_str("## Examples\n\nExamples live under the family fixture tree.\n\n");
    out.push_str("## Fixture Paths\n\nFixture paths are validated by `cargo xtask schemas`.\n\n");
    out.push_str("## Drift Checks\n\nRun `cargo xtask schemas generate --check`.\n");
    out
}

pub(super) fn registry_markdown(family: &str, inputs: &[SourceInput], section: &str) -> String {
    let mut out = markdown(family, inputs);
    out.push_str(&format!(
        "\n## {section}\n\nGenerated from the owner crate schema registry.\n"
    ));
    out
}

/// Renders `docs/reference/cli/commands.md` from the same in-memory command
/// records that produce `docs/reference/cli/commands.json`, plus a Removed
/// Commands section sourced from the removed-surface registry
/// (`xtask/src/schemas/removed_registry.rs`). This is CLAUDE.md's
/// authoritative CLI reference — it must never regress to a header-only stub.
pub(super) fn cli_markdown(
    inputs: &[SourceInput],
    commands: &[Value],
    removed: &[(&str, &str)],
) -> String {
    let mut out = markdown("cli", inputs);
    out.push_str(&format!(
        "\n## Commands\n\nSourced from `docs/reference/cli/commands.json` ({} commands). \
         `Group` is the top-level command family (e.g. `jobs`, `watch`); multi-word \
         `Command` values are `<group> <subcommand>`. `Mutates` and `Auth Scope` mirror \
         the JSON `mutates` / `requires_auth_scope` fields; `Async` marks commands that \
         can return a durable job id instead of completing synchronously.\n\n",
        commands.len()
    ));
    out.push_str(&command_table(commands));
    out.push_str(&format!("\nTotal: {} commands.\n", commands.len()));

    out.push_str(
        "\n## Removed Commands\n\nRemoved in the 7.0.0 clean-break pipeline unification \
         (issue #298) and not present in `commands.json`. Source: \
         `xtask/src/schemas/removed_registry.rs` (`CLI_COMMANDS`). Every removed command \
         now fails fast with a reserved-token error at the parser boundary rather than \
         dispatching; see `crates/axon-core/src/config/source_routing.rs` for the \
         reserved-token rejection table backing that error text.\n\n",
    );
    out.push_str("| Removed command | Replacement |\n|---|---|\n");
    for (name, replacement) in removed {
        out.push_str(&format!("| `axon {name} ...` | `{replacement}` |\n"));
    }
    out.push_str(
        "\nREST and MCP removed-surface entries (routes/actions retired in the same clean \
         break) are tracked by the `openapi` and `mcp` schema families, not duplicated here.\n",
    );
    out
}

/// Renders `docs/reference/cli/axon-help.md`. The target design
/// (`docs/pipeline-unification/delivery/docs-generator-contract.md`'s
/// `cli-help` family) is a real clap `--help` text renderer; that renderer
/// does not exist yet — `CliRegistryCommand` (`xtask/src/schemas/cli_registry.rs`)
/// has no flags/arguments field to render, only path/summary/mutates/async/
/// auth-scope. Until a flag-level renderer lands, this projects the same
/// command registry as a per-command quick-reference instead of an empty
/// stub, and says so explicitly rather than silently under-delivering.
pub(super) fn cli_help_markdown(inputs: &[SourceInput], commands: &[Value]) -> String {
    let mut out = markdown("cli-help", inputs);
    out.push_str(
        "\n## Commands\n\n**Scope note:** this file is a per-command quick reference \
         projected from `docs/reference/cli/commands.json`, not literal `axon <command> \
         --help` output. `CliRegistryCommand` does not carry a flags/arguments registry, \
         so flag-level help text cannot be generated mechanically yet; that requires the \
         clap/help renderer described as the `cli-help` family target in \
         `docs/pipeline-unification/delivery/docs-generator-contract.md`. Run \
         `axon <command> --help` for authoritative flag documentation in the meantime.\n\n",
    );
    out.push_str(&command_table(commands));
    out.push_str(&format!("\nTotal: {} commands.\n", commands.len()));
    out
}

fn command_table(commands: &[Value]) -> String {
    let mut sorted: Vec<&Value> = commands.iter().collect();
    sorted.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let mut out = String::from(
        "| Command | Group | Summary | Mutates | Auth Scope | Async |\n|---|---|---|---|---|---|\n",
    );
    for command in sorted {
        let name = command["name"].as_str().unwrap_or_default();
        let group = command["group"].as_str().unwrap_or_default();
        let summary = command["summary"].as_str().unwrap_or_default();
        let mutates = bool_flag(command, "mutates");
        let auth_scope = command["requires_auth_scope"].as_str().unwrap_or_default();
        let is_async = bool_flag(command, "async");
        out.push_str(&format!(
            "| `{name}` | `{group}` | {summary} | {mutates} | `{auth_scope}` | {is_async} |\n"
        ));
    }
    out
}

fn bool_flag(command: &Value, field: &str) -> &'static str {
    if command[field].as_bool().unwrap_or(false) {
        "yes"
    } else {
        "no"
    }
}

pub(super) fn registry_projection_markdown(
    family: &str,
    command: &str,
    inputs: &[SourceInput],
    section: &str,
) -> String {
    let mut out = generated_header(family, command);
    out.push_str("## Source Inputs\n\n| Path | SHA-256 |\n|---|---|\n");
    for input in inputs {
        out.push_str(&format!("| `{}` | `{}` |\n", input.path, input.checksum));
    }
    out.push_str("\n## Root Shape\n\nGenerated projection from the owning schema family.\n\n");
    out.push_str("## Required Definitions\n\nSee the owning JSON schema artifact.\n\n");
    out.push_str("## Field Tables\n\nGenerated from the same registry model as JSON.\n\n");
    out.push_str(
        "## Enum Tables\n\nGenerated from registry enum projections where applicable.\n\n",
    );
    out.push_str("## Extension Points\n\nExtension points are declared in the source registry when allowed.\n\n");
    out.push_str(
        "## Forbidden Fields\n\nRemoved and secret fields are rejected by schema checks.\n\n",
    );
    out.push_str("## Examples\n\nExamples live under the family fixture tree.\n\n");
    out.push_str("## Fixture Paths\n\nFixture paths are validated by `cargo xtask schemas`.\n\n");
    out.push_str("## Drift Checks\n\nRun `cargo xtask schemas generate --check`.\n");
    out.push_str(&format!(
        "\n## {section}\n\nGenerated from the owner crate schema registry.\n"
    ));
    out
}

pub(super) fn api_markdown(inputs: &[SourceInput]) -> String {
    let mut out = markdown("api", inputs);
    out.push_str("\n## DTO Coverage\n\n| DTO |\n|---|\n");
    for dto in api_defs::api_dto_names() {
        out.push_str(&format!("| `{dto}` |\n"));
    }
    out.push_str("\n## SourceRequest Fixture Matrix\n\n");
    out.push_str(
        "Definition-specific examples are validated from `crates/axon-api/tests/fixtures/schema`.\n\n",
    );
    out.push_str("| Source kind | Fixture |\n|---|---|\n");
    if let Some((_, source_kinds)) = CANONICAL_ENUMS
        .iter()
        .find(|(name, _)| *name == "SourceKind")
    {
        for source_kind in *source_kinds {
            out.push_str(&format!(
                "| `{source_kind}` | `source_request.{source_kind}.valid.json` |\n"
            ));
        }
    }
    out.push_str(
        "\n`memory` is a schema projection for the canonical enum and remains an integration, not a source adapter.\n",
    );
    out
}

pub(super) fn generated_header(family: &str, command: &str) -> String {
    format!(
        "<!-- generated by cargo xtask schemas {command}; do not edit directly -->\n\n# {family} Schema Reference\n\n## Overview\n\nGenerated by `cargo xtask schemas {command}`.\n\n## Generated Artifacts\n\nSee the family contract for declared output paths.\n\n"
    )
}

use super::CANONICAL_ENUMS;
use super::api_defs;
use crate::schemas::source_input::SourceInput;

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

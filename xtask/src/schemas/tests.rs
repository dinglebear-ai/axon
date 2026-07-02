use std::path::Path;

use sha2::{Digest, Sha256};
use tempfile::TempDir;

use super::*;

mod generated_contract_tests;

pub(super) fn fixture_repo() -> TempDir {
    let tmp = TempDir::new().unwrap();
    for path in [
        "crates/axon-api/src/source.rs",
        "crates/axon-api/src/source/boundary.rs",
        "crates/axon-api/src/source/document.rs",
        "crates/axon-api/src/source/lifecycle.rs",
        "crates/axon-api/src/source/listing.rs",
        "crates/axon-api/src/source/enums.rs",
        "crates/axon-api/src/source/graph.rs",
        "crates/axon-api/src/source/ids.rs",
        "crates/axon-api/src/source/job.rs",
        "crates/axon-api/src/source/stage.rs",
        "crates/axon-api/src/source/state.rs",
        "crates/axon-api/src/source/status.rs",
        "crates/axon-api/src/source/vector.rs",
        "crates/axon-api/src/source/capability.rs",
        "crates/axon-error/src/lib.rs",
        "crates/axon-error/src/api_error.rs",
        "crates/axon-error/src/code.rs",
        "crates/axon-error/src/stage.rs",
        "crates/axon-cli/src/lib.rs",
        "crates/axon-core/src/config/types.rs",
        "crates/axon-route/src/capability.rs",
        "crates/axon-web/src/lib.rs",
        "crates/axon-mcp/src/lib.rs",
        "crates/axon-observe/src/lib.rs",
        "crates/axon-graph/src/lib.rs",
        "crates/axon-vectors/src/lib.rs",
        "crates/axon-vectors/src/payload.rs",
        "crates/axon-vectors/src/point.rs",
        "xtask/src/schemas/vector_payload_markdown.rs",
        "docs/pipeline-unification/runtime/error-handling.md",
        "docs/pipeline-unification/surfaces/command-contract.md",
        "docs/pipeline-unification/surfaces/rest-contract.md",
        "docs/pipeline-unification/surfaces/tool-contract.md",
        "docs/pipeline-unification/configuration/config-contract.md",
        "docs/pipeline-unification/runtime/observability-contract.md",
        "docs/pipeline-unification/runtime/schema-contract.md",
        "docs/pipeline-unification/sources/source-graph.md",
        "docs/pipeline-unification/sources/metadata-payload.md",
        "docs/pipeline-unification/sources/chunking-contract.md",
        "docs/pipeline-unification/schemas/vector-payload-schema.md",
        "docs/pipeline-unification/runtime/provider-contract.md",
        "docs/pipeline-unification/sources/adapter-scopes.md",
    ] {
        if needs_real_fixture(path) {
            copy_workspace_fixture(tmp.path(), path);
        } else {
            write_fixture(tmp.path(), path, "fixture");
        }
    }
    write_fixture(
        tmp.path(),
        "crates/axon-jobs/src/migrations/0001.sql",
        "create table jobs(id text);",
    );
    write_fixture(
        tmp.path(),
        "crates/axon-ledger/src/migrations/0001.sql",
        "create table sources(source_id text);",
    );
    tmp
}

fn needs_real_fixture(path: &str) -> bool {
    matches!(
        path,
        "crates/axon-api/src/source/vector.rs"
            | "crates/axon-vectors/src/payload.rs"
            | "crates/axon-vectors/src/point.rs"
            | "docs/pipeline-unification/sources/metadata-payload.md"
            | "docs/pipeline-unification/sources/chunking-contract.md"
            | "docs/pipeline-unification/schemas/vector-payload-schema.md"
    )
}

fn copy_workspace_fixture(root: &Path, path: &str) {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let content = std::fs::read_to_string(repo_root.join(path))
        .unwrap_or_else(|err| panic!("read workspace fixture {path}: {err}"));
    write_fixture(root, path, &content);
}

fn write_fixture(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

pub(super) fn generate(root: &Path) -> Result<()> {
    run(
        root,
        SchemasArgs {
            command: SchemaCommand::Generate(SchemaGenerateArgs::default()),
        },
    )
}

fn check(root: &Path) -> Result<()> {
    run(
        root,
        SchemasArgs {
            command: SchemaCommand::Generate(SchemaGenerateArgs {
                check: true,
                ..SchemaGenerateArgs::default()
            }),
        },
    )
}

#[test]
fn check_fails_when_artifacts_are_missing() {
    let tmp = fixture_repo();
    let err = check(tmp.path()).expect_err("missing artifacts should fail");
    assert!(err.to_string().contains("schema artifacts are stale"));
    assert!(err.to_string().contains("docs/reference/api/schemas.json"));
}

#[test]
fn generate_writes_all_required_family_artifacts() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();

    for path in [
        "docs/reference/api/schemas.json",
        "docs/reference/api/dto.md",
        "docs/reference/api/enums.md",
        "docs/reference/api/errors.schema.json",
        "docs/reference/api/errors.md",
        "docs/reference/cli/commands.json",
        "docs/reference/cli/commands.md",
        "docs/reference/cli/axon-help.md",
        "docs/reference/rest/openapi.json",
        "docs/reference/rest/openapi.md",
        "docs/reference/rest/schemas.md",
        "docs/reference/mcp/tool-schema.json",
        "crates/axon-mcp/tests/golden/tool-schema.json",
        "docs/reference/mcp/pipeline-tool-schema.md",
        "docs/reference/config/config.schema.json",
        "docs/reference/config/env.schema.json",
        "docs/reference/config/config-toml.md",
        "docs/reference/config/env.md",
        "docs/reference/runtime/events.schema.json",
        "docs/reference/runtime/events.md",
        "docs/reference/runtime/database-schema.json",
        "docs/reference/runtime/database-schema.md",
        "docs/reference/sources/graph.schema.json",
        "docs/reference/sources/graph.md",
        "docs/reference/sources/vector-payload.schema.json",
        "docs/reference/sources/vector-payload.md",
        "docs/reference/runtime/provider-capabilities.schema.json",
        "docs/reference/runtime/provider-capabilities.md",
        "docs/reference/sources/adapter-scopes.json",
        "docs/reference/sources/adapter-scopes.md",
    ] {
        assert!(tmp.path().join(path).exists(), "{path} should be generated");
    }
}

#[test]
fn check_passes_after_generation_and_fails_after_stale_artifact() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();
    check(tmp.path()).unwrap();

    assert_stale_after(
        tmp.path(),
        |path| {
            let mut content = std::fs::read_to_string(path).unwrap();
            content.push_str("\n");
            std::fs::write(path, content).unwrap();
        },
        "docs/reference/api/schemas.json differs",
    );
}

#[test]
fn check_mode_does_not_write_existing_artifacts() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();

    let path = tmp.path().join("docs/reference/api/schemas.json");
    let before = std::fs::read_to_string(&path).unwrap();
    check(tmp.path()).unwrap();
    let after = std::fs::read_to_string(&path).unwrap();

    assert_eq!(after, before);
}

#[test]
fn source_input_checksum_matches_fixture_and_drifts_when_source_changes() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();

    let content =
        std::fs::read_to_string(tmp.path().join("docs/reference/api/schemas.json")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&content).unwrap();
    let inputs = value["x-axon"]["source_inputs"].as_array().unwrap();
    let source = inputs
        .iter()
        .find(|input| input["path"] == "crates/axon-api/src/source.rs")
        .unwrap();
    let expected = format!("sha256:{:x}", Sha256::digest(b"fixture"));
    assert_eq!(source["kind"], "rust_module");
    assert_eq!(source["checksum"], expected);

    write_fixture(
        tmp.path(),
        "crates/axon-api/src/source.rs",
        "changed fixture",
    );
    let err = check(tmp.path()).expect_err("source-input checksum drift should fail");
    assert!(
        err.to_string()
            .contains("docs/reference/api/schemas.json differs")
    );
}

#[test]
fn removed_surface_drift_fails_generation() {
    for (path, token) in [
        ("docs/reference/api/schemas.json", "\"EmbedRequest\""),
        ("docs/reference/cli/commands.json", "\"embed\""),
        ("docs/reference/cli/commands.json", "\"code-search\""),
        ("docs/reference/mcp/tool-schema.json", "\"vertical_scrape\""),
        ("docs/reference/mcp/tool-schema.json", "\"crawl\""),
        ("docs/reference/rest/openapi.json", "\"/v1/watch/{id}/run\""),
        ("docs/reference/rest/openapi.json", "\"/v1/scrape\""),
        (
            "docs/reference/config/env.schema.json",
            "\"AXON_MCP_HTTP_TOKEN\"",
        ),
        ("docs/reference/api/schemas.json", "\"url\""),
        ("docs/reference/api/schemas.json", "\"path_prefix\""),
    ] {
        let artifacts = vec![artifact::SchemaArtifact::new(
            path,
            format!("{{\"title\":{token}}}"),
        )];
        let err = registry::check_removed_surface_drift(&artifacts)
            .expect_err("removed surface token should fail");
        assert!(err.to_string().contains("removed public surface token"));
    }
}

#[test]
fn json_report_shape_is_machine_readable() {
    let reports = vec![super::FamilyReport {
        family: SchemaFamily::Api,
        ok: true,
        artifacts_checked: 3,
        drift: Vec::new(),
        warnings: Vec::new(),
    }];
    let value = serde_json::to_value(reports).unwrap();
    assert_eq!(value[0]["family"], "api");
    assert_eq!(value[0]["artifacts_checked"], 3);
}

#[test]
fn json_report_shape_marks_stale_family_as_failed() {
    let report = super::FamilyReport::from_drift(
        SchemaFamily::Api,
        3,
        vec!["docs/reference/api/schemas.json differs".to_string()],
    );
    let value = serde_json::to_value(report).unwrap();
    assert_eq!(value["family"], "api");
    assert_eq!(value["ok"], false);
    assert_eq!(value["drift"][0], "docs/reference/api/schemas.json differs");
}

#[test]
fn json_check_mode_still_reports_stale_artifact_error() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();
    assert_stale_after_with_args(
        tmp.path(),
        |path| std::fs::write(path, "{}\n").unwrap(),
        SchemaGenerateArgs {
            check: true,
            json: true,
            ..SchemaGenerateArgs::default()
        },
        "docs/reference/api/schemas.json differs",
    );
}

#[test]
fn print_and_json_are_mutually_exclusive() {
    let tmp = fixture_repo();
    let err = run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Generate(SchemaGenerateArgs {
                print: true,
                json: true,
                ..SchemaGenerateArgs::default()
            }),
        },
    )
    .expect_err("print plus json should fail");

    assert!(
        err.to_string()
            .contains("--print and --json are mutually exclusive")
    );
}

#[test]
fn single_family_subcommands_reject_family_filter() {
    let tmp = fixture_repo();
    let err = run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Api(SchemaGenerateArgs {
                family: Some(SchemaFamily::Cli),
                ..SchemaGenerateArgs::default()
            }),
        },
    )
    .expect_err("fixed family subcommands should reject --family");

    assert!(err.to_string().contains("--family is only valid"));
}

#[test]
fn config_and_env_schema_artifacts_have_distinct_identity() {
    let tmp = fixture_repo();
    run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Config(SchemaGenerateArgs::default()),
        },
    )
    .unwrap();

    let config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join("docs/reference/config/config.schema.json"))
            .unwrap(),
    )
    .unwrap();
    let env: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join("docs/reference/config/env.schema.json")).unwrap(),
    )
    .unwrap();

    assert_eq!(
        config["$id"],
        "https://axon.local/schemas/config/config.schema.json"
    );
    assert_eq!(
        env["$id"],
        "https://axon.local/schemas/config/env.schema.json"
    );
    assert_ne!(config["title"], env["title"]);
}

#[test]
fn enum_projection_drift_is_scoped_to_each_enum_array() {
    let mut enums = serde_json::Map::new();
    for (name, values) in registry::CANONICAL_ENUMS {
        let values = values
            .iter()
            .copied()
            .filter(|value| !(*name == "SourceKind" && *value == "web"))
            .collect::<Vec<_>>();
        enums.insert((*name).to_string(), serde_json::json!({ "enum": values }));
    }
    let artifact = artifact::SchemaArtifact::new(
        "docs/reference/api/schemas.json",
        serde_json::json!({
            "$defs": { "enums": enums },
            "description": "the word web appears outside SourceKind"
        })
        .to_string(),
    );

    let err = registry::check_enum_projection_drift(&[artifact])
        .expect_err("missing enum value should fail even when value appears elsewhere");
    assert!(err.to_string().contains("SourceKind"));
    assert!(err.to_string().contains("web"));
}

#[test]
fn migration_source_input_kind_uses_normalized_path_components() {
    assert_eq!(
        source_input::source_input_kind("crates/axon-jobs/src/migrations", true),
        source_input::SourceInputKind::SqlMigrationDirectory
    );
    assert_eq!(
        source_input::source_input_kind("crates\\axon-jobs\\src\\migrations", true),
        source_input::SourceInputKind::SqlMigrationDirectory
    );
    assert_eq!(
        source_input::source_input_kind("crates/axon-jobs/src/notmigrations", true),
        source_input::SourceInputKind::RustDirectory
    );
}

fn assert_stale_after(root: &Path, mutate: impl FnOnce(&Path), expected_error_substring: &str) {
    assert_stale_after_with_args(
        root,
        mutate,
        SchemaGenerateArgs {
            check: true,
            ..SchemaGenerateArgs::default()
        },
        expected_error_substring,
    );
}

fn assert_stale_after_with_args(
    root: &Path,
    mutate: impl FnOnce(&Path),
    args: SchemaGenerateArgs,
    expected_error_substring: &str,
) {
    let path = root.join("docs/reference/api/schemas.json");
    mutate(&path);

    let err = run(
        root,
        SchemasArgs {
            command: SchemaCommand::Generate(args),
        },
    )
    .expect_err("stale artifact should fail");

    assert!(err.to_string().contains("schema artifacts are stale"));
    assert!(err.to_string().contains(expected_error_substring));
}

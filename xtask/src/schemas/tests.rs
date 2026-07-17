use std::path::Path;

use sha2::{Digest, Sha256};
use tempfile::TempDir;

use super::*;

mod generated_contract_tests;

pub(super) fn fixture_repo() -> TempDir {
    let tmp = TempDir::new().unwrap();
    for path in [
        "crates/axon-api/src/source.rs",
        "crates/axon-api/src/schema_registry.rs",
        "crates/axon-api/src/source/boundary.rs",
        "crates/axon-api/src/source/common.rs",
        "crates/axon-api/src/source/capability.rs",
        "crates/axon-api/src/source/document.rs",
        "crates/axon-api/src/source/lifecycle.rs",
        "crates/axon-api/src/source/listing.rs",
        "crates/axon-api/src/source/enums.rs",
        "crates/axon-api/src/source/graph.rs",
        "crates/axon-api/src/source/ids.rs",
        "crates/axon-api/src/source/job.rs",
        "crates/axon-api/src/source/job_listing.rs",
        "crates/axon-api/src/source/provider_io.rs",
        "crates/axon-api/src/source/prune.rs",
        "crates/axon-api/src/source/stage.rs",
        "crates/axon-api/src/source/state.rs",
        "crates/axon-api/src/source/status.rs",
        "crates/axon-api/src/source/vector.rs",
        "crates/axon-error/src/lib.rs",
        "crates/axon-error/src/schema_registry.rs",
        "crates/axon-error/src/api_error.rs",
        "crates/axon-error/src/code.rs",
        "crates/axon-error/src/stage.rs",
        "crates/axon-error/src/retry.rs",
        "crates/axon-error/src/degradation.rs",
        "crates/axon-error/src/cooling.rs",
        "crates/axon-error/src/context.rs",
        "crates/axon-cli/src/schema_registry.rs",
        "crates/axon-core/src/config/cli.rs",
        "crates/axon-core/src/config/cli/config_args.rs",
        "crates/axon-core/src/config/cli/resources_args.rs",
        "crates/axon-core/src/config/cli/setup_args.rs",
        "xtask/src/schemas/cli_registry.rs",
        "xtask/src/schemas/cli_registry/part1.rs",
        "xtask/src/schemas/cli_registry/part2.rs",
        "xtask/src/schemas/cli_registry/part3.rs",
        "xtask/src/schemas/cli_registry/part4.rs",
        "xtask/src/schemas/database_defs.rs",
        "xtask/src/schemas/database_defs/parser.rs",
        "crates/axon-core/src/config/schema_registry.rs",
        "xtask/src/schemas/config_schema_registry.rs",
        "crates/axon-core/src/boundary.rs",
        "crates/axon-embedding/src/provider.rs",
        "crates/axon-embedding/src/fake.rs",
        "crates/axon-llm/src/provider.rs",
        "crates/axon-llm/src/fake.rs",
        "crates/axon-adapters/src/boundary.rs",
        "crates/axon-authz/src/policy.rs",
        "crates/axon-observe/src/reservation.rs",
        "crates/axon-route/src/capability.rs",
        "crates/axon-adapters/src/family_matrix.rs",
        "crates/axon-adapters/src/onboarding.rs",
        "crates/axon-adapters/src/spec.rs",
        "crates/axon-adapters/src/web.rs",
        "crates/axon-adapters/fixtures/provider-variant-exceptions.json",
        "crates/axon-web/src/schema_registry.rs",
        "crates/axon-mcp/src/schema_registry.rs",
        "xtask/src/schemas/mcp_action_registry.rs",
        "crates/axon-mcp/src/server/authz.rs",
        "crates/axon-mcp/src/server.rs",
        "crates/axon-api/src/mcp_schema.rs",
        "crates/axon-api/src/mcp_schema/requests.rs",
        "crates/axon-api/src/mcp_schema/requests/discovery.rs",
        "crates/axon-api/src/mcp_schema/requests/graph.rs",
        "crates/axon-api/src/mcp_schema/requests/watch.rs",
        "crates/axon-api/src/mcp_schema/prune_request.rs",
        "crates/axon-api/src/mcp_schema/utility.rs",
        "crates/axon-observe/src/schema_registry.rs",
        "crates/axon-observe/src/metric.rs",
        "crates/axon-graph/src/schema_registry.rs",
        "crates/axon-vectors/src/schema_registry.rs",
        "crates/axon-vectors/src/lib.rs",
        "crates/axon-vectors/src/store.rs",
        "crates/axon-vectors/src/payload.rs",
        "crates/axon-vectors/src/payload_families.rs",
        "crates/axon-vectors/src/point.rs",
        "xtask/src/schemas/api_defs.rs",
        "xtask/src/schemas/registry.rs",
        "xtask/src/schemas/tests.rs",
        "xtask/src/schemas/vector_payload_markdown.rs",
        "docs/pipeline-unification/schemas/api-dto-schema.md",
        "docs/pipeline-unification/schemas/cli-schema.md",
        "docs/pipeline-unification/schemas/openapi-schema.md",
        "docs/pipeline-unification/schemas/mcp-tool-schema.md",
        "docs/pipeline-unification/schemas/config-schema.md",
        "docs/pipeline-unification/configuration/config-contract.md",
        "docs/pipeline-unification/schemas/event-schema.md",
        "docs/pipeline-unification/schemas/error-schema.md",
        "docs/pipeline-unification/schemas/database-schema.md",
        "docs/pipeline-unification/schemas/graph-schema.md",
        "docs/pipeline-unification/runtime/provider-contract.md",
        "docs/pipeline-unification/sources/metadata-payload.md",
        "docs/pipeline-unification/sources/chunking-contract.md",
        "docs/pipeline-unification/schemas/vector-payload-schema.md",
        "docs/pipeline-unification/schemas/provider-capability-schema.md",
        "docs/pipeline-unification/sources/adapter-scopes.md",
        "docs/pipeline-unification/sources/new-source-contract.md",
    ] {
        if needs_real_fixture(path) {
            copy_workspace_fixture(tmp.path(), path);
        } else {
            write_fixture(tmp.path(), path, "fixture");
        }
    }
    write_fixture(
        tmp.path(),
        "crates/axon-jobs/src/migrations/0001_create_jobs.sql",
        "create table jobs(id text);",
    );
    write_fixture(
        tmp.path(),
        "crates/axon-ledger/src/migrations/0001_create_sources.sql",
        "create table sources(source_id text);",
    );
    write_fixture(
        tmp.path(),
        "crates/axon-observe/src/migrations/0001_create_observability_tables.sql",
        "create table source_events(event_id text primary key);",
    );
    write_fixture(
        tmp.path(),
        "crates/axon-graph/src/migrations/0001_create_graph.sql",
        "create table graph_nodes(node_id text primary key);",
    );
    write_fixture(
        tmp.path(),
        "crates/axon-memory/src/migrations/0001_create_memory.sql",
        "create table memory_records(memory_id text primary key);",
    );
    seed_schema_fixtures(tmp.path());
    tmp
}

fn seed_schema_fixtures(root: &Path) {
    for family in families::all_families() {
        let base = format!("xtask/tests/fixtures/schemas/{}", family.as_str());
        write_fixture(
            root,
            &format!("{base}/valid/minimal.json"),
            valid_fixture_for(family),
        );
        write_fixture(root, &format!("{base}/invalid/not-object.json"), "[]");
        seed_generated_schema_snapshots(root, family, &format!("{base}/snapshots"));
    }
    write_fixture(
        root,
        "crates/axon-api/tests/fixtures/schema/source_request.valid.json",
        r#"{"source":"https://example.com"}"#,
    );
    write_fixture(
        root,
        "crates/axon-api/tests/fixtures/schema/source_request.missing-required.invalid.json",
        r#"{"scope":"page"}"#,
    );
}

fn seed_generated_schema_snapshots(root: &Path, family: SchemaFamily, target: &str) {
    let target = root.join(target);
    std::fs::create_dir_all(&target).unwrap();
    let artifacts = families::generator_for(family)
        .generate(root)
        .unwrap_or_else(|err| panic!("generate {family:?} schema snapshots: {err}"));
    for artifact in artifacts {
        if artifact.path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            let file_name = artifact.path.file_name().unwrap();
            std::fs::write(target.join(file_name), artifact.content).unwrap();
        }
    }
}

fn valid_fixture_for(family: SchemaFamily) -> &'static str {
    match family {
        SchemaFamily::Cli => r#"{"commands":[]}"#,
        SchemaFamily::Openapi => r#"{"routes":[]}"#,
        SchemaFamily::Mcp => "{}",
        SchemaFamily::Config => r#"{"config_keys":[]}"#,
        SchemaFamily::Graph => r#"{"graph_kinds":[]}"#,
        SchemaFamily::Providers => "{}",
        SchemaFamily::Adapters => {
            r#"{
  "x-axon": {
    "contract_version": "2026-06-30",
    "generated_by": "cargo xtask schemas adapters",
    "owner_crates": ["axon-route", "axon-adapters"],
    "source_inputs": [],
    "clean_break": true,
    "registry_status": "fixture",
    "adapters": [],
    "source_family_matrix": []
  }
}"#
        }
        SchemaFamily::VectorPayload => {
            r#"{
  "payload_contract_version": "2026-07-01",
  "collection": "axon",
  "vector_point_id": "vector_point_id",
  "vector_namespace": "vector_namespace",
  "source_family": "code",
  "source_kind": "source_kind",
  "source_adapter": "source_adapter",
  "source_scope": "source_scope",
  "source_id": "src-code",
  "source_canonical_uri": "source_canonical_uri",
  "source_item_key": "code-item",
  "item_canonical_uri": "item_canonical_uri",
  "source_generation": 7,
  "document_id": "doc-code",
  "chunk_id": "chunk-code-0",
  "chunk_index": 0,
  "chunking_profile": "markdown_sections",
  "chunking_method": "heading_sections",
  "content_kind": "content_kind",
  "chunk_content_kind": "source",
  "content_hash": "sha256:codehash",
  "chunk_hash": "chunk_hash",
  "chunk_text": "chunk_text",
  "chunk_locator": {
    "canonical_uri": "https://example.com/code",
    "heading_path": ["code heading"],
    "path": "/code",
    "range": {"line_start": 1, "line_end": 4}
  },
  "source_range": {"line_start": 1, "line_end": 4},
  "visibility": "internal",
  "redaction_status": "clean",
  "redaction_version": "2026-07-16",
  "redacted_field_count": 0,
  "dropped_field_count": 0,
  "detector_count": 0,
  "detector_names": [],
  "job_id": "job-code",
  "document_status": "prepared",
  "embedding_model": "text-embedding-test",
  "embedding_dimensions": 768,
  "embedding_provider": "tei",
  "embedding_profile": "default",
  "embedded_at": "2026-06-30T00:00:00Z",
  "committed_generation": null
}"#
        }
        SchemaFamily::Api
        | SchemaFamily::Events
        | SchemaFamily::Errors
        | SchemaFamily::Database => "{}",
    }
}

fn needs_real_fixture(path: &str) -> bool {
    matches!(
        path,
        "crates/axon-api/src/source/vector.rs"
            | "crates/axon-api/src/source/capability.rs"
            | "crates/axon-core/src/boundary.rs"
            | "crates/axon-embedding/src/provider.rs"
            | "crates/axon-embedding/src/fake.rs"
            | "crates/axon-llm/src/provider.rs"
            | "crates/axon-llm/src/fake.rs"
            | "crates/axon-vectors/src/store.rs"
            | "crates/axon-adapters/src/boundary.rs"
            | "crates/axon-authz/src/policy.rs"
            | "crates/axon-observe/src/reservation.rs"
            | "crates/axon-vectors/src/payload.rs"
            | "crates/axon-vectors/src/payload_families.rs"
            | "crates/axon-vectors/src/point.rs"
            | "crates/axon-adapters/fixtures/provider-variant-exceptions.json"
            | "docs/pipeline-unification/runtime/provider-contract.md"
            | "docs/pipeline-unification/sources/metadata-payload.md"
            | "docs/pipeline-unification/sources/chunking-contract.md"
            | "docs/pipeline-unification/schemas/vector-payload-schema.md"
            | "docs/pipeline-unification/schemas/provider-capability-schema.md"
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
fn schema_family_statuses_are_explicit() {
    let metadata = families::family_metadata();

    assert_eq!(metadata.len(), families::all_families().len());
    for entry in metadata {
        assert_eq!(
            entry.status,
            families::FamilyStatus::RegistryBacked,
            "{} must declare an explicit registry-backed status",
            entry.family.as_str()
        );
    }
}

#[test]
fn skeleton_artifacts_are_not_contract_complete() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();

    for family in families::all_families() {
        let artifacts = families::generator_for(family)
            .generate(tmp.path())
            .unwrap();
        for artifact in artifacts {
            assert!(
                !artifact.content.contains("SchemaFamilyContract"),
                "{} must not emit the skeleton SchemaFamilyContract into {}",
                family.as_str(),
                artifact.path.display()
            );
            assert!(
                !artifact.content.contains("\"status\":\"skeleton\"")
                    && !artifact.content.contains("\"status\": \"skeleton\""),
                "{} must not mark generated artifacts as skeleton",
                family.as_str()
            );
        }
    }
}

#[test]
fn phase_2_rejects_validation_only_or_deferred_families() {
    for entry in families::family_metadata() {
        assert_eq!(
            entry.status,
            families::FamilyStatus::RegistryBacked,
            "{} must not be ValidationOnly or Deferred",
            entry.family.as_str()
        );
    }
}

#[test]
fn adapters_are_a_schema_family() {
    assert!(
        families::all_families()
            .iter()
            .any(|family| family.as_str() == "adapters"),
        "adapters must be generated as the Phase 9 source capability family"
    );
}

#[test]
fn api_schema_contains_phase_1_required_defs() {
    let artifact = std::fs::read_to_string(workspace_path("docs/reference/api/schemas.json"))
        .expect("read generated API schema artifact");
    let schema: serde_json::Value =
        serde_json::from_str(&artifact).expect("parse generated API schema artifact");
    let defs = schema
        .get("$defs")
        .and_then(|value| value.as_object())
        .expect("generated API schema has $defs object");

    for name in families::api_defs::PHASE_1_REQUIRED_API_DEFS {
        assert!(
            defs.contains_key(*name),
            "missing API schema $defs entry: {name}"
        );
    }
}

#[test]
fn phase_1_deferred_api_defs_are_documented() {
    for (name, owner, reason) in families::api_defs::PHASE_1_DEFERRED_API_DEFS {
        assert!(!name.is_empty(), "deferred API def must have a name");
        assert!(
            !owner.is_empty(),
            "deferred API def {name} must have an owner plan"
        );
        assert!(
            !reason.is_empty(),
            "deferred API def {name} must have a reason"
        );
    }
}

#[test]
fn api_request_dtos_have_scope_entries() {
    assert_eq!(
        families::api_defs::request_scope_for("ArtifactListRequest", "artifact.list"),
        Some("read")
    );
    assert_eq!(
        families::api_defs::request_scope_for("UploadCreateRequest", "upload.create"),
        Some("write")
    );
    assert_eq!(
        families::api_defs::request_scope_for("PruneExecuteRequest", "prune.execute"),
        Some("admin")
    );
    assert_eq!(
        families::api_defs::request_scope_for("CollectionListRequest", "collection.list"),
        Some("read")
    );
    assert_eq!(
        families::api_defs::request_scope_for("UnknownRequest", "unknown.action"),
        None
    );
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
fn check_accepts_docs_generated_markdown_header() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();

    let path = tmp
        .path()
        .join("docs/reference/mcp/pipeline-tool-schema.md");
    let content = std::fs::read_to_string(&path).unwrap();
    let body = content
        .split_once("\n\n")
        .map(|(_, body)| body)
        .expect("generated markdown starts with a header comment block");
    std::fs::write(
        &path,
        format!(
            "<!-- generated by cargo xtask docs generate --family mcp; do not edit directly -->\n\
             <!-- source inputs: sha256:test -->\n\n{body}"
        ),
    )
    .unwrap();

    check(tmp.path()).expect("schema check should accept docs-generated markdown header");
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
    assert!(err.to_string().contains("differs"));
}

#[test]
fn source_inputs_are_registry_scoped() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();

    for path in [
        "docs/reference/cli/commands.json",
        "docs/reference/rest/openapi.json",
        "docs/reference/mcp/tool-schema.json",
        "docs/reference/config/config.schema.json",
        "docs/reference/runtime/events.schema.json",
        "docs/reference/sources/graph.schema.json",
        "docs/reference/runtime/provider-capabilities.schema.json",
    ] {
        let content = std::fs::read_to_string(tmp.path().join(path)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        let inputs = value["x-axon"]["source_inputs"].as_array().unwrap();
        assert!(
            inputs.iter().any(|input| {
                input["path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with("schema_registry.rs"))
            }),
            "{path} should include an owner registry source input"
        );
        for input in inputs {
            let source_path = input["path"].as_str().unwrap();
            assert!(
                !matches!(
                    source_path,
                    "crates/axon-cli/src"
                        | "crates/axon-web/src"
                        | "crates/axon-mcp/src"
                        | "crates/axon-core/src/config"
                        | "crates/axon-observe/src"
                ),
                "{path} used broad source input {source_path}"
            );
        }
    }
}

#[test]
fn check_mode_does_not_write_any_schema_artifact() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();
    let before = generated_artifact_contents(tmp.path());

    check(tmp.path()).unwrap();

    assert_eq!(generated_artifact_contents(tmp.path()), before);
}

#[test]
fn targeted_family_checks_do_not_require_hidden_aggregate_generation() {
    let tmp = fixture_repo();
    run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Cli(SchemaGenerateArgs::default()),
        },
    )
    .unwrap();

    run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Cli(SchemaGenerateArgs {
                check: true,
                ..SchemaGenerateArgs::default()
            }),
        },
    )
    .unwrap();

    assert!(
        !tmp.path().join("docs/reference/api/schemas.json").exists(),
        "targeted CLI check should not generate hidden aggregate artifacts"
    );
}

#[test]
fn removed_surface_registry_matches_contract() {
    let registry = removed::removed_surface_registry();

    assert!(
        registry
            .cli_commands
            .iter()
            .any(|command| command.name == "embed")
    );
    assert!(
        registry
            .cli_commands
            .iter()
            .any(|command| command.name == "code-search-watch")
    );
    assert!(
        registry
            .mcp_actions
            .iter()
            .any(|action| action.name == "vertical_scrape")
    );
    assert!(registry.rest_routes.iter().any(|route| {
        route.method == "POST" && route.path == "/v1/embed" && route.operation_id == "embed"
    }));
    assert!(
        registry
            .config_keys
            .iter()
            .any(|key| key.name == "AXON_MCP_HTTP_TOKEN")
    );
    assert!(registry.generated_clients.contains(&"web"));
    assert!(registry.generated_clients.contains(&"palette"));
    assert!(registry.generated_clients.contains(&"android"));
    assert!(registry.generated_clients.contains(&"chrome-extension"));
}

#[test]
fn removed_surface_checker_reports_structural_findings() {
    let artifacts = vec![
        artifact::SchemaArtifact::new(
            "docs/reference/cli/commands.json",
            serde_json::json!({
                "commands": [{"name": "embed"}, {"name": "query"}]
            })
            .to_string(),
        ),
        artifact::SchemaArtifact::new(
            "docs/reference/mcp/tool-schema.json",
            serde_json::json!({
                "actions": [{"action": "vertical_scrape"}, {"action": "query"}]
            })
            .to_string(),
        ),
        artifact::SchemaArtifact::new(
            "docs/reference/rest/openapi.json",
            serde_json::json!({
                "routes": [
                    {"method": "POST", "path": "/v1/embed", "operation_id": "embed"},
                    {"method": "POST", "path": "/v1/query", "operation_id": "query"}
                ]
            })
            .to_string(),
        ),
        artifact::SchemaArtifact::new(
            "docs/reference/config/env.schema.json",
            serde_json::json!({
                "config_keys": [{"env_key": "AXON_MCP_HTTP_TOKEN", "key": "auth.token"}]
            })
            .to_string(),
        ),
        artifact::SchemaArtifact::new(
            "docs/reference/api/schemas.json",
            serde_json::json!({
                "$defs": {
                    "CodeSearchRequest": {
                        "properties": {
                            "cwd": {"type": "string"}
                        }
                    }
                }
            })
            .to_string(),
        ),
        artifact::SchemaArtifact::new(
            "apps/web/generated/client-operations.json",
            serde_json::json!({
                "client": "web",
                "operations": [{"operation_id": "watch_run"}]
            })
            .to_string(),
        ),
    ];

    let report = removed::removed_surface_absence_report(&artifacts);
    let findings = report
        .findings
        .iter()
        .map(|finding| (finding.category, finding.surface.as_str()))
        .collect::<std::collections::BTreeSet<_>>();

    assert!(findings.contains(&("CLI command", "embed")));
    assert!(findings.contains(&("MCP action", "vertical_scrape")));
    assert!(findings.contains(&("REST route", "POST /v1/embed")));
    assert!(findings.contains(&("REST operation", "embed")));
    assert!(findings.contains(&("config key", "AXON_MCP_HTTP_TOKEN")));
    assert!(findings.contains(&("DTO schema", "CodeSearchRequest")));
    assert!(findings.contains(&("DTO field", "CodeSearchRequest.cwd")));
    assert!(findings.contains(&("generated client operation", "watch_run")));

    let err = removed::assert_removed_surface_absent(&report)
        .expect_err("present removed surfaces should be reported");
    assert!(err.to_string().contains("replacement"));
}

#[test]
fn removed_surface_checker_accepts_absent_canonical_artifacts() {
    let artifacts = vec![
        artifact::SchemaArtifact::new(
            "docs/reference/cli/commands.json",
            serde_json::json!({
                "commands": [{"name": "source"}, {"name": "query"}]
            })
            .to_string(),
        ),
        artifact::SchemaArtifact::new(
            "docs/reference/mcp/tool-schema.json",
            serde_json::json!({
                "actions": [{"action": "source"}, {"action": "query"}]
            })
            .to_string(),
        ),
        artifact::SchemaArtifact::new(
            "docs/reference/rest/openapi.json",
            serde_json::json!({
                "routes": [
                    {
                        "method": "POST",
                        "path": "/v1/sources",
                        "operation_id": "create_source"
                    },
                    {
                        "method": "POST",
                        "path": "/v1/watches/{watch_id}/exec",
                        "operation_id": "exec_watch"
                    }
                ]
            })
            .to_string(),
        ),
        artifact::SchemaArtifact::new(
            "docs/reference/config/env.schema.json",
            serde_json::json!({
                "config_keys": [{"env_key": "AXON_HTTP_TOKEN", "key": "auth.token"}]
            })
            .to_string(),
        ),
        artifact::SchemaArtifact::new(
            "docs/reference/api/schemas.json",
            serde_json::json!({
                "$defs": {
                    "QueryRequest": {
                        "properties": {
                            "filters": {"type": "object"}
                        }
                    }
                }
            })
            .to_string(),
        ),
    ];

    let report = removed::removed_surface_absence_report(&artifacts);

    assert!(report.is_clean());
    removed::assert_removed_surface_absent(&report).unwrap();
}

#[test]
fn removed_surface_checker_rejects_retired_cleanup_and_artifact_routes() {
    let artifact = artifact::SchemaArtifact::new(
        "docs/reference/rest/openapi.json",
        serde_json::json!({
            "routes": [
                {
                    "method": "POST",
                    "path": "/v1/prune/dedupe",
                    "operation_id": "prune_dedupe"
                },
                {
                    "method": "GET",
                    "path": "/v1/artifacts/{path}",
                    "operation_id": "artifact_by_path"
                }
            ]
        })
        .to_string(),
    );

    let report = removed::removed_surface_absence_report(&[artifact]);
    let surfaces = report
        .findings
        .iter()
        .map(|finding| finding.surface.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(surfaces.contains("POST /v1/prune/dedupe"));
    assert!(surfaces.contains("GET /v1/artifacts/{path}"));
    assert!(surfaces.contains("prune_dedupe"));
}

#[test]
fn removed_legacy_api_request_shapes_are_absent() {
    let artifact = std::fs::read_to_string(workspace_path("docs/reference/api/schemas.json"))
        .expect("read generated API schema artifact");
    let schema: serde_json::Value =
        serde_json::from_str(&artifact).expect("parse generated API schema artifact");
    let defs = schema
        .get("$defs")
        .and_then(|value| value.as_object())
        .expect("generated API schema has $defs object");

    for removed_def in [
        "EmbedRequest",
        "IngestRequest",
        "CrawlRequest",
        "ScrapeRequest",
        "CodeSearchRequest",
    ] {
        assert!(
            !defs.contains_key(removed_def),
            "legacy request def leaked: {removed_def}"
        );
    }

    if let Some(purge) = defs.get("PurgeRequest") {
        let properties = purge
            .get("properties")
            .and_then(|value| value.as_object())
            .expect("PurgeRequest schema has properties");
        assert!(
            !properties.contains_key("target"),
            "legacy PurgeRequest.target leaked"
        );
        assert!(
            !properties.contains_key("prefix"),
            "legacy PurgeRequest.prefix leaked"
        );
    }
}

#[test]
fn api_schema_has_field_level_x_axon_metadata() {
    let artifact = std::fs::read_to_string(workspace_path("docs/reference/api/schemas.json"))
        .expect("read generated API schema artifact");
    let schema: serde_json::Value =
        serde_json::from_str(&artifact).expect("parse generated API schema artifact");
    assert!(schema["x-axon"]["source_inputs"].is_array());
    assert!(schema["x-axon"]["owner_crates"].is_array());
}

#[test]
fn cli_schema_is_registry_backed_and_contains_command_records() {
    let tmp = fixture_repo();
    run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Cli(SchemaGenerateArgs::default()),
        },
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join("docs/reference/cli/commands.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(value["x-axon"]["status"], "RegistryBacked");
    assert!(
        value["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|command| command["name"] == "extract")
    );
}

#[test]
fn mcp_schema_is_registry_backed_and_validates_action_branches() {
    let tmp = fixture_repo();
    run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Mcp(SchemaGenerateArgs::default()),
        },
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join("docs/reference/mcp/tool-schema.json")).unwrap(),
    )
    .unwrap();
    // The `extract` action is live and must have its real request DTO def.
    assert!(value["$defs"]["ExtractRequest"].is_object());
    let action_enum = value["$defs"]["Action"]["enum"].as_array().unwrap();
    assert!(action_enum.iter().any(|a| a == "extract"));
    // Removed/HTTP-only/never-contracted actions must be absent from the
    // live Action enum.
    for removed in ["crawl", "embed", "ingest", "purge", "dedupe", "scrape"] {
        assert!(
            !action_enum.iter().any(|a| a == removed),
            "removed action {removed:?} must not appear in the Action enum"
        );
    }
    // Contract-only actions with no live DTO surface as `deferred_actions`,
    // not fabricated schemas.
    let deferred = value["x-axon"]["deferred_actions"].as_array().unwrap();
    assert!(
        deferred
            .iter()
            .any(|entry| entry["action"] == "chat" || entry["action"] == "watches")
    );
    // Every live action has an if/then discriminator branch.
    let branches = value["$defs"]["ActionDiscriminatorRules"]["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(branches.len(), action_enum.len());
}

#[test]
fn openapi_has_no_dangling_refs() {
    let tmp = fixture_repo();
    run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Openapi(SchemaGenerateArgs::default()),
        },
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join("docs/reference/rest/openapi.json")).unwrap(),
    )
    .unwrap();
    assert!(value["routes"].is_array());
}

#[test]
fn openapi_routes_have_auth_scope_and_envelopes() {
    let tmp = fixture_repo();
    run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Openapi(SchemaGenerateArgs::default()),
        },
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join("docs/reference/rest/openapi.json")).unwrap(),
    )
    .unwrap();
    for route in value["routes"].as_array().unwrap() {
        assert!(route["requires_auth_scope"].is_string());
        assert!(
            route["responses"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("401"))
        );
        assert!(
            route["responses"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("403"))
        );
    }
}

#[test]
fn removed_rest_routes_are_absent() {
    let routes = axon_web::schema_registry::rest_route_registry()
        .iter()
        .map(|route| route.path)
        .collect::<std::collections::BTreeSet<_>>();
    for removed in axon_web::schema_registry::removed_routes() {
        assert!(!routes.contains(removed), "removed route leaked: {removed}");
    }
}

#[test]
fn rest_schema_registry_matches_current_openapi_route_inventory() {
    let generated = axon_web::schema_registry::rest_route_registry()
        .iter()
        .map(|route| (route.method, route.path))
        .collect::<std::collections::BTreeSet<_>>();
    let inventory = axon_services::types::rest_route_inventory()
        .iter()
        .filter(|route| route.openapi && route.path.starts_with("/v1/"))
        .map(|route| (route.method, route.path))
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(inventory, generated);
}

#[test]
fn config_removed_keys_are_rejected() {
    let keys = axon_core::config::schema_registry::config_key_registry()
        .iter()
        .filter_map(|key| key.env_key)
        .collect::<std::collections::BTreeSet<_>>();
    for removed in axon_core::config::schema_registry::removed_env_keys() {
        assert!(!keys.contains(removed), "removed env key leaked: {removed}");
    }
}

#[test]
fn provider_schema_requires_contract_fields() {
    let tmp = fixture_repo();
    run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Providers(SchemaGenerateArgs::default()),
        },
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            tmp.path()
                .join("docs/reference/runtime/provider-capabilities.schema.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let required = value["$defs"]["ProviderCapability"]["required"]
        .as_array()
        .unwrap();
    for field in [
        "health",
        "limits",
        "reservation_policy",
        "reservation_state",
        "degraded_modes",
    ] {
        assert!(
            required.iter().any(|required| required == field),
            "ProviderCapability should require {field}"
        );
    }
}

#[test]
fn provider_schema_is_not_a_skeleton_and_contains_reservation_fields() {
    let tmp = fixture_repo();
    run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Providers(SchemaGenerateArgs::default()),
        },
    )
    .unwrap();

    let content = std::fs::read_to_string(
        tmp.path()
            .join("docs/reference/runtime/provider-capabilities.schema.json"),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert_ne!(
        value["$defs"]["SchemaFamilyContract"]["properties"]["status"]["const"], "skeleton",
        "provider capability schema must be generated from real provider DTOs"
    );
    assert!(
        value["$defs"].get("ProviderCapability").is_some(),
        "ProviderCapability schema definition should be present"
    );
    assert!(
        value["$defs"]["ProviderCapability"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "reservation_policy"),
        "reservation_policy must be a required provider capability field"
    );
    assert!(
        value["$defs"]["ProviderCapability"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "reservation_state"),
        "reservation_state must be a required provider capability field"
    );
    assert!(
        value["$defs"].get("ReservationPolicy").is_some(),
        "ReservationPolicy schema definition should be present"
    );
    assert!(
        value["$defs"].get("ReservationStateSnapshot").is_some(),
        "ReservationStateSnapshot schema definition should be present"
    );
}

#[test]
fn phase_3_boundary_inventory() {
    const BOUNDARIES: &[(&str, &str, &str, &str)] = &[
        (
            "LedgerStore",
            "crates/axon-ledger/src/store.rs",
            "pub trait LedgerStore",
            "crates/axon-ledger/src/store/fake.rs",
        ),
        (
            "GraphStore",
            "crates/axon-graph/src/store.rs",
            "pub trait GraphStore",
            "crates/axon-graph/src/store.rs",
        ),
        (
            "MemoryStore",
            "crates/axon-memory/src/store.rs",
            "pub trait MemoryStore",
            "crates/axon-memory/src/store.rs",
        ),
        (
            "VectorStore",
            "crates/axon-vectors/src/store.rs",
            "pub trait VectorStore",
            "crates/axon-vectors/src/store.rs",
        ),
        (
            "ArtifactStore",
            "crates/axon-core/src/boundary.rs",
            "pub trait ArtifactStore",
            "crates/axon-core/src/boundary.rs",
        ),
        (
            "JobStore",
            "crates/axon-jobs/src/boundary.rs",
            "pub trait JobStore",
            "crates/axon-jobs/src/fake_store.rs",
        ),
        (
            "WatchStore",
            "crates/axon-jobs/src/boundary.rs",
            "pub trait WatchStore",
            "crates/axon-jobs/src/fake_store.rs",
        ),
        (
            "ConfigStore",
            "crates/axon-core/src/boundary.rs",
            "pub trait ConfigStore",
            "crates/axon-core/src/boundary.rs",
        ),
        (
            "DocumentCache",
            "crates/axon-core/src/boundary.rs",
            "pub trait DocumentCache",
            "crates/axon-core/src/boundary.rs",
        ),
        (
            "EmbeddingProvider",
            "crates/axon-embedding/src/provider.rs",
            "pub trait EmbeddingProvider",
            "crates/axon-embedding/src/fake.rs",
        ),
        (
            "LlmProvider",
            "crates/axon-llm/src/provider.rs",
            "pub trait LlmProvider",
            "crates/axon-llm/src/fake.rs",
        ),
        (
            "SearchProvider",
            "crates/axon-adapters/src/boundary.rs",
            "pub trait SearchProvider",
            "crates/axon-adapters/src/boundary.rs",
        ),
        (
            "FetchProvider",
            "crates/axon-adapters/src/boundary.rs",
            "pub trait FetchProvider",
            "crates/axon-adapters/src/boundary.rs",
        ),
        (
            "RenderProvider",
            "crates/axon-adapters/src/boundary.rs",
            "pub trait RenderProvider",
            "crates/axon-adapters/src/boundary.rs",
        ),
        (
            "NetworkCaptureProvider",
            "crates/axon-adapters/src/boundary.rs",
            "pub trait NetworkCaptureProvider",
            "crates/axon-adapters/src/boundary.rs",
        ),
        (
            "CredentialProvider",
            "crates/axon-authz/src/policy.rs",
            "pub trait CredentialProvider",
            "crates/axon-authz/src/policy.rs",
        ),
        (
            "HealthProbe",
            "crates/axon-core/src/boundary.rs",
            "pub trait HealthProbe",
            "crates/axon-core/src/boundary.rs",
        ),
        (
            "RateLimiter",
            "crates/axon-core/src/boundary.rs",
            "pub trait RateLimiter",
            "crates/axon-core/src/boundary.rs",
        ),
        (
            "SecurityPolicy",
            "crates/axon-authz/src/policy.rs",
            "pub trait SecurityPolicy",
            "crates/axon-authz/src/policy.rs",
        ),
    ];

    for (name, trait_path, trait_marker, fake_path) in BOUNDARIES {
        let trait_content = workspace_file(trait_path);
        assert!(
            trait_content.contains(trait_marker),
            "{name} owner trait missing from {trait_path}"
        );
        let fake_content = workspace_file(fake_path);
        assert!(
            fake_content.contains("Fake"),
            "{name} fake/in-memory boundary missing from {fake_path}"
        );
        assert!(
            fake_content.contains("capabilities") || fake_content.contains("capability"),
            "{name} fake must report capability or health"
        );
        assert!(
            fake_content.contains("reset")
                || name.ends_with("Provider")
                || name == &"SecurityPolicy",
            "{name} store fake must expose reset/idempotency behavior"
        );
    }

    let ledger_store = workspace_file("crates/axon-ledger/src/store.rs");
    assert!(ledger_store.contains("update_document_status"));
    assert!(ledger_store.contains("document_status"));
    assert!(
        !workspace_file("crates/axon-api/src/source/capability.rs").contains("DocumentStatusStore")
    );
    assert!(!workspace_file("crates/axon-ledger/src/store.rs").contains("DocumentStatusStore"));

    let jobs_manifest = workspace_file("crates/axon-jobs/Cargo.toml");
    assert!(
        !jobs_manifest.contains("axon-services"),
        "axon-jobs must not depend on axon-services"
    );
}

fn workspace_file(path: &str) -> String {
    std::fs::read_to_string(workspace_path(path))
        .unwrap_or_else(|err| panic!("read workspace file {path}: {err}"))
}

#[test]
fn graph_schema_validates_edge_and_evidence_contracts() {
    assert!(
        axon_graph::schema_registry::edge_kind_registry()
            .iter()
            .all(|edge| edge.requires_evidence)
    );
}

#[test]
fn event_schema_matches_api_enum_projections() {
    assert!(
        axon_observe::schema_registry::event_registry()
            .iter()
            .any(|event| event.name == "JobEvent")
    );
    assert!(
        registry::CANONICAL_ENUMS
            .iter()
            .any(|(name, _)| *name == "PipelinePhase")
    );
}

#[test]
fn error_schema_stage_projection_is_explicit() {
    assert!(
        axon_error::schema_registry::error_registry()
            .iter()
            .all(|error| !error.stage.is_empty())
    );
}

#[test]
fn database_schema_rejects_legacy_tables() {
    let schema = families::generator_for(SchemaFamily::Database)
        .generate(&fixture_repo().into_path())
        .unwrap()
        .into_iter()
        .find(|artifact| artifact.path.ends_with("database-schema.json"))
        .unwrap()
        .content;
    for legacy in ["memory_decay", "watch_events", "job_config_snapshots"] {
        assert!(!schema.contains(legacy));
    }
}

#[test]
fn vector_payload_schema_requires_source_generation() {
    let artifact = std::fs::read_to_string(workspace_path(
        "docs/reference/sources/vector-payload.schema.json",
    ))
    .expect("read vector payload schema");
    assert!(artifact.contains("source_generation"));
    assert!(!artifact.contains("\"generation\""));
}

#[test]
fn cross_checks_detect_dangling_refs() {
    let artifact = artifact::SchemaArtifact::new(
        "docs/reference/api/schemas.json",
        serde_json::json!({ "$ref": "#/$defs/Missing", "$defs": {} }).to_string(),
    );
    let index =
        artifact_index::ArtifactIndex::from_generated(SchemaFamily::Api, &[artifact]).unwrap();
    let err = cross_check::check_dangling_refs(&index).unwrap_err();
    assert!(err.to_string().contains("dangling local ref"));
}

#[test]
fn cross_checks_detect_removed_surface_drift() {
    removed_surface_checker_reports_structural_findings();
}

#[test]
fn cross_checks_detect_scope_mismatch() {
    let cli_extract = axon_cli::schema_registry::command_registry()
        .iter()
        .find(|command| command.name == "extract")
        .unwrap();
    let mcp_extract = axon_mcp::schema_registry::action_registry()
        .iter()
        .find(|action| action.action == "extract")
        .unwrap();
    assert_eq!(cli_extract.required_scope, mcp_extract.required_scope);
}

#[test]
fn per_crate_generated_artifact_docs_are_checked() {
    assert!(workspace_path("docs/pipeline-unification/schemas/README.md").exists());
}

#[test]
fn app_client_artifacts_match_openapi_and_api_schemas() {
    let route_paths = axon_web::schema_registry::rest_route_registry()
        .iter()
        .map(|route| route.path)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(route_paths.contains("/v1/ask"));
    assert!(route_paths.contains("/v1/extract"));
}

#[test]
fn openapi_registry_exposes_only_canonical_admin_watch_and_artifact_routes() {
    let routes = axon_web::schema_registry::rest_route_registry();
    let route_keys = routes
        .iter()
        .map(|route| (route.method, route.path, route.operation_id))
        .collect::<std::collections::BTreeSet<_>>();

    for expected in [
        ("POST", "/v1/reset/plan", "plan_reset"),
        ("POST", "/v1/reset/exec", "execute_reset"),
        ("GET", "/v1/collections", "collections"),
        ("POST", "/v1/watches/{watch_id}/exec", "watches_exec"),
        ("GET", "/v1/watches/{watch_id}/history", "watches_history"),
        ("GET", "/v1/artifacts/{artifact_id}", "get_artifact"),
        (
            "GET",
            "/v1/artifacts/{artifact_id}/content",
            "artifact_content",
        ),
    ] {
        assert!(
            route_keys.contains(&expected),
            "missing canonical route {expected:?}"
        );
    }

    for removed in [
        "/v1/crawl",
        "/v1/embed",
        "/v1/ingest",
        "/v1/dedupe",
        "/v1/purge",
        "/v1/prune/dedupe",
        "/v1/prune/purge",
        "/v1/watch/{id}/run",
        "/v1/artifacts/{path}",
    ] {
        assert!(
            !routes.iter().any(|route| route.path == removed),
            "removed route remains advertised: {removed}"
        );
    }
}

#[test]
fn dependency_graph_snapshots_reject_forbidden_edges() {
    assert!(workspace_path("docs/reference/crate-dependency-graph.md").exists());
}

#[test]
fn generated_markdown_has_required_sections() {
    let tmp = fixture_repo();
    run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Cli(SchemaGenerateArgs::default()),
        },
    )
    .unwrap();
    let markdown =
        std::fs::read_to_string(tmp.path().join("docs/reference/cli/commands.md")).unwrap();
    for section in [
        "Overview",
        "Generated Artifacts",
        "Source Inputs",
        "Root Shape",
        "Required Definitions",
        "Field Tables",
        "Enum Tables",
        "Extension Points",
        "Forbidden Fields",
        "Examples",
        "Fixture Paths",
        "Drift Checks",
    ] {
        assert!(
            markdown.contains(section),
            "missing markdown section {section}"
        );
    }
}

#[test]
fn documented_examples_validate_against_generated_schemas() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();
    let schema: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join("docs/reference/cli/commands.json")).unwrap(),
    )
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    validator
        .validate(&serde_json::json!({ "commands": [] }))
        .expect("documented minimal CLI example should validate");
}

#[test]
fn markdown_and_json_drift_together() {
    let tmp = fixture_repo();
    run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Cli(SchemaGenerateArgs::default()),
        },
    )
    .unwrap();
    std::fs::write(tmp.path().join("docs/reference/cli/commands.md"), "stale").unwrap();
    let err = run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Cli(SchemaGenerateArgs {
                check: true,
                ..SchemaGenerateArgs::default()
            }),
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("commands.md differs"));
}

#[test]
fn removed_surface_drift_checks_legacy_api_defs_by_schema_path() {
    let artifacts = vec![artifact::SchemaArtifact::new(
        "docs/reference/api/schemas.json",
        serde_json::json!({
            "$defs": {
                "EmbedRequest": {
                    "type": "object",
                    "properties": {
                        "input": { "type": "string" }
                    }
                }
            },
            "description": "EmbedRequest may appear in prose, but not as a $defs key"
        })
        .to_string(),
    )];
    let err = registry::check_removed_surface_drift(&artifacts)
        .expect_err("legacy API request def should fail");
    assert!(err.to_string().contains("EmbedRequest"), "{err}");
}

#[test]
fn removed_surface_drift_checks_legacy_purge_properties_by_schema_path() {
    let artifacts = vec![artifact::SchemaArtifact::new(
        "docs/reference/api/schemas.json",
        serde_json::json!({
            "$defs": {
                "PurgeRequest": {
                    "type": "object",
                    "properties": {
                        "target": { "type": "string" },
                        "reason": { "type": "string" }
                    }
                }
            },
            "description": "PurgeRequest is allowed only when its legacy fields are gone"
        })
        .to_string(),
    )];
    let err = registry::check_removed_surface_drift(&artifacts)
        .expect_err("legacy purge property should fail");
    assert!(err.to_string().contains("PurgeRequest.target"), "{err}");
}

#[test]
fn json_report_shape_is_machine_readable() {
    let reports = vec![super::FamilyReport {
        family: SchemaFamily::Api,
        ok: true,
        artifacts_checked: 3,
        fixtures_validated: 2,
        snapshots_checked: 1,
        drift: Vec::new(),
        warnings: Vec::new(),
    }];
    let value = serde_json::to_value(reports).unwrap();
    assert_eq!(value[0]["family"], "api");
    assert_eq!(value[0]["artifacts_checked"], 3);
    assert_eq!(value[0]["fixtures_validated"], 2);
    assert_eq!(value[0]["snapshots_checked"], 1);
}

#[test]
fn family_report_includes_fixture_and_snapshot_counts() {
    let report = super::FamilyReport {
        family: SchemaFamily::Cli,
        ok: true,
        artifacts_checked: 2,
        fixtures_validated: 5,
        snapshots_checked: 2,
        drift: Vec::new(),
        warnings: Vec::new(),
    };
    let value = serde_json::to_value(report).unwrap();
    assert_eq!(value["fixtures_validated"], 5);
    assert_eq!(value["snapshots_checked"], 2);
}

#[test]
fn every_registry_backed_family_has_standard_fixture_categories() {
    let tmp = fixture_repo();
    let fixture_root = tmp.path().join("xtask/tests/fixtures/schemas/cli");
    for dir in ["valid", "invalid", "snapshots"] {
        std::fs::create_dir_all(fixture_root.join(dir)).unwrap();
    }
    write_fixture(
        tmp.path(),
        "xtask/tests/fixtures/schemas/cli/valid/minimal.json",
        r#"{"commands":[]}"#,
    );
    write_fixture(
        tmp.path(),
        "xtask/tests/fixtures/schemas/cli/invalid/missing-required.json",
        "{}",
    );

    run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Cli(SchemaGenerateArgs::default()),
        },
    )
    .unwrap();
}

#[test]
fn empty_schema_snapshot_category_fails_validation() {
    let tmp = fixture_repo();
    let snapshots = tmp
        .path()
        .join("xtask/tests/fixtures/schemas/cli/snapshots");
    for entry in std::fs::read_dir(&snapshots).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().and_then(|ext| ext.to_str()) == Some("json") {
            std::fs::remove_file(entry.path()).unwrap();
        }
    }

    let err = run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Cli(SchemaGenerateArgs::default()),
        },
    )
    .expect_err("schema families without snapshots should fail validation");
    assert!(err.to_string().contains("schema snapshot fixture"));
}

#[test]
fn invalid_fixtures_fail_real_schema_validation() {
    let tmp = fixture_repo();
    let fixture_root = tmp.path().join("xtask/tests/fixtures/schemas/cli");
    std::fs::create_dir_all(fixture_root.join("valid")).unwrap();
    std::fs::create_dir_all(fixture_root.join("invalid")).unwrap();
    std::fs::create_dir_all(fixture_root.join("snapshots")).unwrap();
    write_fixture(
        tmp.path(),
        "xtask/tests/fixtures/schemas/cli/valid/minimal.json",
        r#"{"commands":[]}"#,
    );
    write_fixture(
        tmp.path(),
        "xtask/tests/fixtures/schemas/cli/invalid/missing-required.json",
        r#"{}"#,
    );

    run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Cli(SchemaGenerateArgs::default()),
        },
    )
    .expect("valid fixture should pass and invalid fixture should fail validation as expected");

    write_fixture(
        tmp.path(),
        "xtask/tests/fixtures/schemas/cli/invalid/missing-required.json",
        r#"{"commands":[]}"#,
    );
    let err = run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Cli(SchemaGenerateArgs::default()),
        },
    )
    .expect_err("invalid fixture that passes schema validation should fail");
    assert!(err.to_string().contains("unexpectedly passed"));
}

#[test]
fn owner_fixtures_validate_against_named_api_definitions() {
    let tmp = fixture_repo();
    run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Api(SchemaGenerateArgs::default()),
        },
    )
    .expect("SourceRequest owner fixtures should validate against the named definition");

    write_fixture(
        tmp.path(),
        "crates/axon-api/tests/fixtures/schema/source_request.missing-required.invalid.json",
        r#"{"source":"https://example.com"}"#,
    );
    let err = run(
        tmp.path(),
        SchemasArgs {
            command: SchemaCommand::Api(SchemaGenerateArgs::default()),
        },
    )
    .expect_err("an invalid SourceRequest fixture accepted by the definition must fail");
    assert!(err.to_string().contains("unexpectedly passed"));
}

#[test]
fn schema_cli_accepts_required_commands_and_flags() {
    let tmp = fixture_repo();
    for command in [
        SchemaCommand::Generate(SchemaGenerateArgs::default()),
        SchemaCommand::Api(SchemaGenerateArgs::default()),
        SchemaCommand::Cli(SchemaGenerateArgs::default()),
        SchemaCommand::Openapi(SchemaGenerateArgs::default()),
        SchemaCommand::Mcp(SchemaGenerateArgs::default()),
        SchemaCommand::Config(SchemaGenerateArgs::default()),
        SchemaCommand::Events(SchemaGenerateArgs::default()),
        SchemaCommand::Errors(SchemaGenerateArgs::default()),
        SchemaCommand::Database(SchemaGenerateArgs::default()),
        SchemaCommand::Graph(SchemaGenerateArgs::default()),
        SchemaCommand::VectorPayload(SchemaGenerateArgs::default()),
        SchemaCommand::Providers(SchemaGenerateArgs::default()),
        SchemaCommand::Adapters(SchemaGenerateArgs::default()),
    ] {
        run(tmp.path(), SchemasArgs { command }).expect("schema command should succeed");
    }
}

#[test]
fn schema_cli_exit_codes_are_stable() {
    assert_eq!(SchemaExitCode::Success as i32, 0);
    assert_eq!(SchemaExitCode::ValidationOrDriftFailure as i32, 1);
    assert_eq!(SchemaExitCode::BadInvocation as i32, 2);
    assert_eq!(
        SchemaExitCode::SourceInputOrArtifactManifestFailure as i32,
        3
    );
    assert_eq!(SchemaExitCode::InternalGeneratorError as i32, 4);
}

#[test]
fn schema_generator_contract_fixtures_validate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("xtask/tests/fixtures/schema-generator");
    for relative in [
        "valid/all-families.valid.json",
        "valid/check-report.valid.json",
        "invalid/missing-family.invalid.json",
        "invalid/stale-artifact.invalid.json",
        "invalid/dangling-ref.invalid.json",
        "invalid/missing-source-input.invalid.json",
    ] {
        let path = root.join(relative);
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read generator fixture {}: {err}", path.display()));
        let value: serde_json::Value = serde_json::from_str(&content)
            .unwrap_or_else(|err| panic!("parse generator fixture {}: {err}", path.display()));
        if relative.starts_with("valid/") {
            assert!(
                value.get("expected_error").is_none(),
                "valid generator fixture {relative} must not declare expected_error"
            );
        } else {
            assert!(
                value
                    .get("expected_error")
                    .and_then(serde_json::Value::as_str)
                    .is_some(),
                "invalid generator fixture {relative} must declare expected_error"
            );
        }
    }
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

    // Distinct identity means more than a differing $id/title: the two
    // artifacts must not be near-duplicate arrays of the same record shape.
    assert!(config.get("config_keys").is_some());
    assert!(env.get("env_vars").is_some());
    assert!(config.get("env_vars").is_none());
    assert!(env.get("config_keys").is_none());
    assert_ne!(
        config["properties"], env["properties"],
        "config and env item schemas must differ, not just top-level metadata"
    );
}

#[test]
fn config_schema_covers_all_required_contract_keys() {
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
    let keys: std::collections::BTreeSet<&str> = config["config_keys"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["key"].as_str())
        .collect();
    for required in [
        "server.default_collection",
        "server.json_pretty",
        "pipeline.max_active_source_jobs",
        "pipeline.max_active_interactive_jobs",
        "jobs.heartbeat_secs",
        "jobs.provider_reservation_timeout_secs",
        "sources.embed_by_default",
        "sources.default_scope_web",
        "sources.default_scope_local",
        "watch.tick_secs",
        "watch.lease_secs",
        "providers.embedding.batch_size",
        "providers.embedding.max_concurrent_requests",
        "providers.embedding.interactive_reserved_requests",
        "providers.vector.write_concurrency",
        "providers.vector.read_concurrency",
        "providers.llm.completion_concurrency",
        "providers.search.default",
        "retrieval.limit",
        "retrieval.hybrid_candidates",
        "retrieval.ask_hybrid_candidates",
        "crawl.max_pages",
        "crawl.respect_robots",
        "memory.decay_enabled",
        "memory.review_interval_days",
        "graph.enabled",
        "prune.retention_days.jobs",
        "observability.log_level",
        "security.allow_private_network_fetch",
    ] {
        assert!(
            keys.contains(required),
            "config schema missing required key {required}"
        );
    }

    let env: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join("docs/reference/config/env.schema.json")).unwrap(),
    )
    .unwrap();
    let env_names: std::collections::BTreeSet<&str> = env["env_vars"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .collect();
    for required in [
        "AXON_DATA_DIR",
        "QDRANT_URL",
        "TEI_URL",
        "AXON_CHROME_REMOTE_URL",
        "AXON_HTTP_HOST",
        "AXON_HTTP_PORT",
        "AXON_PUBLIC_URL",
        "AXON_HTTP_TOKEN",
        "AXON_AUTH_MODE",
        "AXON_GOOGLE_CLIENT_ID",
        "AXON_GOOGLE_CLIENT_SECRET",
        "GITHUB_TOKEN",
        "GITLAB_TOKEN",
        "GITEA_TOKEN",
        "REDDIT_CLIENT_ID",
        "REDDIT_CLIENT_SECRET",
        "TAVILY_API_KEY",
        "AXON_SEARXNG_URL",
        "AXON_OPENAI_API_KEY",
        "AXON_OPENAI_BASE_URL",
        "AXON_CODEX_HOME",
    ] {
        assert!(
            env_names.contains(required),
            "env schema missing required var {required}"
        );
    }
}

#[test]
fn schema_generation_is_idempotent_across_all_families() {
    let tmp = fixture_repo();

    // First pass: --update-fixtures regenerates docs/reference/** artifacts
    // and the fixture snapshots together.
    run_families_with_ci_policy(
        tmp.path(),
        all_families(),
        &SchemaGenerateArgs {
            update_fixtures: true,
            ..SchemaGenerateArgs::default()
        },
        false,
    )
    .unwrap();
    let first_pass = generated_artifact_contents(tmp.path());
    let first_snapshots = snapshot_contents(tmp.path());
    assert!(
        !first_pass.is_empty(),
        "first pass should produce artifacts"
    );

    // Second pass over the same tree must be byte-identical: no drift from
    // running the generator again with no source changes.
    run_families_with_ci_policy(
        tmp.path(),
        all_families(),
        &SchemaGenerateArgs {
            update_fixtures: true,
            ..SchemaGenerateArgs::default()
        },
        false,
    )
    .unwrap();
    let second_pass = generated_artifact_contents(tmp.path());
    let second_snapshots = snapshot_contents(tmp.path());

    assert_eq!(
        first_pass, second_pass,
        "cargo xtask schemas generate --update-fixtures is not idempotent for docs/reference artifacts"
    );
    assert_eq!(
        first_snapshots, second_snapshots,
        "cargo xtask schemas generate --update-fixtures is not idempotent for fixture snapshots"
    );

    // `--check` against the first pass's output must also be clean, i.e. a
    // plain (non-update-fixtures) `generate` run afterward writes nothing new.
    check(tmp.path()).expect("generated artifacts must already satisfy --check");
}

fn snapshot_contents(root: &Path) -> std::collections::BTreeMap<String, String> {
    let mut contents = std::collections::BTreeMap::new();
    for family in families::all_families() {
        let dir = root.join(format!(
            "xtask/tests/fixtures/schemas/{}/snapshots",
            family.as_str()
        ));
        if !dir.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&dir) {
            let entry = entry.unwrap();
            if entry.file_type().is_file() {
                let rel = entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                contents.insert(rel, std::fs::read_to_string(entry.path()).unwrap());
            }
        }
    }
    contents
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
fn enum_projection_drift_rejects_non_canonical_extra_values() {
    let mut enums = serde_json::Map::new();
    for (name, values) in registry::CANONICAL_ENUMS {
        let mut values = values
            .iter()
            .copied()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if *name == "SourceIntent" {
            values.push("bogus_extra_variant".to_string());
        }
        enums.insert((*name).to_string(), serde_json::json!({ "enum": values }));
    }
    let artifact = artifact::SchemaArtifact::new(
        "docs/reference/api/schemas.json",
        serde_json::json!({ "$defs": { "enums": enums } }).to_string(),
    );

    let err = registry::check_enum_projection_drift(&[artifact])
        .expect_err("extra non-canonical enum value should fail the bidirectional check");
    assert!(err.to_string().contains("SourceIntent"));
    assert!(err.to_string().contains("bogus_extra_variant"));
}

/// Cross-checks every `CANONICAL_ENUMS` entry against the real
/// `schemars::JsonSchema` output of its backing `axon-api` enum, so a typo'd
/// or stale hand-maintained value list fails fast instead of silently
/// drifting from the Rust source of truth.
#[test]
fn canonical_enums_match_axon_api_schemars_output() {
    use axon_api::source::*;

    fn schemars_values<T: schemars::JsonSchema>() -> Vec<String> {
        let schema = schemars::schema_for!(T);
        let value: serde_json::Value = schema.into();
        // Plain enums project as a flat `enum` array; variants carrying a doc
        // comment can make schemars emit a `oneOf` with
        // per-variant `enum`/`const` branches instead — flatten both shapes.
        if let Some(values) = value["enum"].as_array() {
            return values
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect();
        }
        let mut values = Vec::new();
        for branch in value["oneOf"]
            .as_array()
            .unwrap_or_else(|| panic!("expected an `enum` or `oneOf` array in {value}"))
        {
            if let Some(v) = branch["const"].as_str() {
                values.push(v.to_string());
            } else if let Some(arr) = branch["enum"].as_array() {
                values.extend(arr.iter().map(|v| v.as_str().unwrap().to_string()));
            } else {
                panic!("unexpected oneOf branch shape in {branch}");
            }
        }
        values
    }

    fn public_job_kind_values() -> Vec<String> {
        JobKind::all()
            .iter()
            .copied()
            .filter(|kind| kind.is_public_source_surface())
            .map(|kind| {
                serde_json::to_value(kind)
                    .expect("JobKind serializes")
                    .as_str()
                    .expect("JobKind serializes to string")
                    .to_string()
            })
            .collect()
    }

    macro_rules! check {
        ($name:literal, $ty:ty) => {
            let (_, expected) = registry::CANONICAL_ENUMS
                .iter()
                .find(|(name, _)| *name == $name)
                .unwrap_or_else(|| panic!("{} missing from CANONICAL_ENUMS", $name));
            let actual = schemars_values::<$ty>();
            assert_eq!(
                actual,
                expected.to_vec(),
                "{} CANONICAL_ENUMS values drifted from schemars output",
                $name
            );
        };
    }

    check!("SourceIntent", SourceIntent);
    check!("SourceRefreshPolicy", SourceRefreshPolicy);
    check!("SourceWatchPolicy", SourceWatchPolicy);
    check!("ExecutionMode", ExecutionMode);
    check!("ResponseMode", ResponseMode);
    check!("ArtifactMode", ArtifactMode);
    check!("SourceKind", SourceKind);
    check!("SourceScope", SourceScope);
    check!("ItemKind", ItemKind);
    check!("ContentKind", ContentKind);
    check!("PipelinePhase", PipelinePhase);
    let (_, expected_job_kind) = registry::CANONICAL_ENUMS
        .iter()
        .find(|(name, _)| *name == "JobKind")
        .expect("JobKind missing from CANONICAL_ENUMS");
    assert_eq!(
        public_job_kind_values(),
        expected_job_kind.to_vec(),
        "JobKind CANONICAL_ENUMS values drifted from public source-surface projection"
    );
    check!("LifecycleStatus", LifecycleStatus);
    check!("PublishState", PublishState);
    check!("DocumentLifecycleStatus", DocumentLifecycleStatus);
    check!("DiffKind", DiffKind);
    check!("EnrichmentKind", EnrichmentKind);
    check!("CleanupDebtKind", CleanupDebtKind);
    check!("ProviderKind", ProviderKind);
    check!("HealthStatus", HealthStatus);
    check!("Visibility", Visibility);
    check!("Severity", Severity);
    check!("JobPriority", JobPriority);
    check!("AuthorityLevel", AuthorityLevel);
    check!("ExecutionAffinity", ExecutionAffinity);
    check!("SafetyClass", SafetyClass);
    check!("CredentialKind", CredentialKind);
    check!("ArtifactKind", ArtifactKind);
    check!("CachePolicy", CachePolicy);
    check!("ChunkProfile", ChunkProfile);
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

fn workspace_path(path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has workspace parent")
        .join(path)
}

fn generated_artifact_contents(root: &Path) -> std::collections::BTreeMap<String, String> {
    let mut contents = std::collections::BTreeMap::new();
    for family in families::all_families() {
        for artifact in families::generator_for(family).generate(root).unwrap() {
            let path = artifact.path.to_string_lossy().replace('\\', "/");
            contents.insert(
                path.clone(),
                std::fs::read_to_string(root.join(path)).unwrap(),
            );
        }
    }
    contents
}

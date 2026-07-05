use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{SourceAdapterSpec, source_family_matrix};

const PACKS: &[&str] = &[
    "resolve",
    "manifest",
    "source-documents",
    "source-jobs",
    "auth",
    "degraded",
    "provider-failure",
    "metadata",
];

const REQUIRED_FIXTURE_FIELDS: &[&str] = &[
    "source_id",
    "source_canonical_uri",
    "adapter",
    "adapter_version",
    "source_item_key",
    "item_canonical_uri",
    "metadata_family",
    "graph",
    "redaction_status",
];

const REQUIRED_VECTOR_PAYLOAD_FIELDS: &[&str] = &[
    "payload_contract_version",
    "collection",
    "source_family",
    "source_kind",
    "source_adapter",
    "source_scope",
    "source_id",
    "source_canonical_uri",
    "source_item_key",
    "item_canonical_uri",
    "source_generation",
    "committed_generation",
    "document_id",
    "chunk_id",
    "chunk_text",
    "chunk_locator",
    "source_range",
    "vector_namespace",
    "visibility",
    "redaction_status",
    "job_id",
    "embedding_batch_id",
    "document_status",
    "embedding_model",
    "embedding_dimensions",
    "embedding_provider",
    "embedding_profile",
    "embedded_at",
    "vector_point_id",
    "content_kind",
    "content_hash",
    "chunk_hash",
];

#[test]
fn fixture_packs_required_families_have_required_fixture_packs() {
    for spec in source_family_matrix()
        .iter()
        .filter(|spec| spec.is_source_adapter)
    {
        for pack in PACKS {
            assert_fixture_dir(spec, pack);
        }
        assert_parse_fixture(spec);
        assert_graph_fixture(spec);
        assert_metadata_fixture(spec);
        assert_vector_payload_fixture(spec);
    }
}

#[test]
fn fixture_packs_required_family_fixtures_have_contract_fields() {
    for spec in source_family_matrix()
        .iter()
        .filter(|spec| spec.is_source_adapter)
    {
        for pack in PACKS {
            let dir = adapter_fixture_root(spec).join(pack);
            for entry in std::fs::read_dir(&dir)
                .unwrap_or_else(|err| panic!("read fixture dir {}: {err}", dir.display()))
            {
                let entry = entry.expect("read fixture entry");
                if entry.path().extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let value = read_json(&entry.path());
                assert_common_fixture_fields(spec, &entry.path(), &value);
                if pack == &"source-jobs" {
                    assert_source_job_fixture(&entry.path(), &value);
                }
                if entry
                    .path()
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.contains("invalid"))
                {
                    assert!(
                        value.get("expected_error_code").is_some(),
                        "{} missing expected_error_code",
                        entry.path().display()
                    );
                }
                if pack == &"degraded" || pack == &"provider-failure" {
                    assert!(
                        value.get("expected_degraded_code").is_some()
                            || value.get("expected_error_code").is_some(),
                        "{} missing degraded/error code",
                        entry.path().display()
                    );
                }
            }
        }
    }
}

fn assert_fixture_dir(spec: &SourceAdapterSpec, pack: &str) {
    let dir = adapter_fixture_root(spec).join(pack);
    assert!(dir.is_dir(), "missing fixture dir {}", dir.display());
    assert!(
        dir.read_dir()
            .unwrap_or_else(|err| panic!("read fixture dir {}: {err}", dir.display()))
            .any(|entry| entry
                .expect("fixture entry")
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                == Some("json")),
        "fixture dir {} has no json fixtures",
        dir.display()
    );
}

fn assert_parse_fixture(spec: &SourceAdapterSpec) {
    assert_json_file(
        repo_root()
            .join("crates/axon-parse/fixtures")
            .join(format!("{}.json", spec.adapter)),
    );
}

fn assert_graph_fixture(spec: &SourceAdapterSpec) {
    assert_json_file(
        repo_root()
            .join("crates/axon-graph/fixtures")
            .join(format!("{}.json", spec.adapter)),
    );
}

fn assert_metadata_fixture(spec: &SourceAdapterSpec) {
    assert_json_file(adapter_fixture_root(spec).join("metadata/public-fields.valid.json"));
}

fn assert_vector_payload_fixture(spec: &SourceAdapterSpec) {
    let path = repo_root()
        .join("crates/axon-vectors/tests/fixtures/payload")
        .join(format!("{}.valid.json", spec.adapter));
    let value = read_json(&path);
    for field in REQUIRED_VECTOR_PAYLOAD_FIELDS {
        assert!(
            value.get(field).is_some(),
            "{} missing {field}",
            path.display()
        );
    }
    assert_eq!(
        value["source_adapter"],
        spec.adapter,
        "{} source_adapter drift",
        path.display()
    );
}

fn assert_common_fixture_fields(spec: &SourceAdapterSpec, path: &Path, value: &Value) {
    for field in REQUIRED_FIXTURE_FIELDS {
        assert!(
            value.get(field).is_some(),
            "{} missing {field}",
            path.display()
        );
    }
    assert_eq!(
        value["adapter"],
        spec.adapter,
        "{} adapter drift",
        path.display()
    );
    assert_eq!(
        value["adapter_version"],
        spec.version,
        "{} adapter version drift",
        path.display()
    );
    assert!(
        value["graph"]
            .get("required_fact_kinds")
            .and_then(Value::as_array)
            .is_some_and(|facts| !facts.is_empty()),
        "{} missing required graph declarations",
        path.display()
    );
}

fn assert_source_job_fixture(path: &Path, value: &Value) {
    for field in [
        "job_id",
        "stage_sequence",
        "generation_publish_state",
        "counts",
        "cleanup_debt_behavior",
        "provider_degradation_behavior",
    ] {
        assert!(
            value.get(field).is_some(),
            "{} missing {field}",
            path.display()
        );
    }
}

fn assert_json_file(path: PathBuf) {
    let value = read_json(&path);
    assert!(
        value.is_object(),
        "{} must be a JSON object",
        path.display()
    );
}

fn read_json(path: &Path) -> Value {
    let bytes = std::fs::read(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()))
}

fn adapter_fixture_root(spec: &SourceAdapterSpec) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(spec.adapter)
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf()
}

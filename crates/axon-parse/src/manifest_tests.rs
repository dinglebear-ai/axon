use axon_api::source::*;
use uuid::Uuid;

use crate::manifest::{dependency_facts, dependency_parse_result};
use crate::parser::ParseInput;

fn input(path: &str, kind: ContentKind, text: &str) -> ParseInput {
    ParseInput {
        job_id: JobId::new(Uuid::from_u128(11)),
        stage_id: StageId::new(Uuid::from_u128(12)),
        requested_parser: None,
        document: SourceDocument {
            document_id: DocumentId::from(format!("doc_{path}")),
            source_id: SourceId::from("src_repo"),
            source_item_key: SourceItemKey::from(path),
            canonical_uri: format!("file:///repo/{path}"),
            content_kind: kind,
            content: ContentRef::InlineText {
                text: text.to_string(),
            },
            metadata: MetadataMap::new(),
            title: None,
            language: None,
            path: Some(path.to_string()),
            mime_type: None,
            structured_payload: None,
            artifact_id: None,
            chunk_hints: Vec::new(),
            parser_hints: Vec::new(),
        },
    }
}

#[test]
fn extracts_manifest_dependencies_and_graph_candidates() {
    let samples = [
        (
            "Cargo.toml",
            ContentKind::Toml,
            "[dependencies]\ntokio = \"1\"\nserde = { version = \"1\", features = [\"derive\"] }\n",
            vec!["tokio", "serde"],
        ),
        (
            "package.json",
            ContentKind::Json,
            r#"{"dependencies":{"react":"19"},"devDependencies":{"vite":"7"}}"#,
            vec!["react", "vite"],
        ),
        (
            "requirements.txt",
            ContentKind::PlainText,
            "fastapi==0.115\nuvicorn>=0.30\n",
            vec!["fastapi", "uvicorn"],
        ),
        (
            "pyproject.toml",
            ContentKind::Toml,
            "[project]\ndependencies = [\"httpx>=0.27\", \"pydantic\"]\n",
            vec!["httpx", "pydantic"],
        ),
        (
            "go.mod",
            ContentKind::PlainText,
            "module example.com/axon\n\nrequire (\n  github.com/spf13/cobra v1.8.1\n  golang.org/x/sync v0.7.0\n)\n",
            vec!["github.com/spf13/cobra", "golang.org/x/sync"],
        ),
        (
            "pom.xml",
            ContentKind::Xml,
            "<project><dependencies><dependency><groupId>org.slf4j</groupId><artifactId>slf4j-api</artifactId><version>2.0.13</version></dependency></dependencies></project>",
            vec!["org.slf4j:slf4j-api"],
        ),
    ];

    for (path, kind, text, expected_names) in samples {
        let (facts, candidates) = dependency_facts(&input(path, kind, text));
        let names: Vec<_> = facts.iter().map(|fact| fact.name.as_str()).collect();
        assert_eq!(names, expected_names);
        assert_eq!(candidates.len(), facts.len());
        assert!(facts.iter().all(|fact| fact.fact_kind == "dependency"));
        assert!(candidates.iter().all(|candidate| {
            candidate.kind == "manifest_dependency" && !candidate.evidence.is_empty()
        }));
    }
}

#[test]
fn extracts_heuristic_yaml_iac_resources_and_graph_candidates() {
    let (facts, candidates) = dependency_facts(&input(
        "deploy.yaml",
        ContentKind::Yaml,
        "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: axon-web\n---\napiVersion: v2\nkind: Chart\nname: axon\n",
    ));

    let names: Vec<_> = facts.iter().map(|fact| fact.name.as_str()).collect();
    assert_eq!(names, vec!["Deployment/axon-web", "Chart/axon"]);
    assert!(facts.iter().all(|fact| {
        fact.fact_kind == "iac_resource" && fact.parser_method == "yaml_iac_heuristic"
    }));
    assert!(
        candidates.iter().all(|candidate| {
            candidate.kind == "iac_resource" && !candidate.evidence.is_empty()
        })
    );
}

#[test]
fn extracts_cargo_workspace_members_and_features() {
    let (facts, candidates) = dependency_facts(&input(
        "Cargo.toml",
        ContentKind::Toml,
        "[workspace]\nmembers = [\"crates/a\", \"crates/b\"]\n\n[features]\ndefault = [\"a\"]\n",
    ));

    let members: Vec<_> = facts
        .iter()
        .filter(|fact| fact.fact_kind == "workspace_member")
        .map(|fact| fact.name.as_str())
        .collect();
    assert_eq!(members, vec!["crates/a", "crates/b"]);

    let features: Vec<_> = facts
        .iter()
        .filter(|fact| fact.fact_kind == "manifest_feature")
        .map(|fact| fact.name.as_str())
        .collect();
    assert_eq!(features, vec!["default"]);

    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.kind == "workspace_member")
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.kind == "manifest_feature")
    );
}

#[test]
fn extracts_npm_toolchain_scripts() {
    let (facts, candidates) = dependency_facts(&input(
        "package.json",
        ContentKind::Json,
        r#"{"scripts":{"build":"tsc","test":"vitest"}}"#,
    ));

    let scripts: Vec<_> = facts
        .iter()
        .filter(|fact| fact.fact_kind == "toolchain_script")
        .map(|fact| fact.name.as_str())
        .collect();
    assert_eq!(scripts.len(), 2);
    assert!(scripts.contains(&"build"));
    assert!(scripts.contains(&"test"));
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.kind == "toolchain_script")
    );
}

#[test]
fn extracts_pyproject_optional_dependency_extras() {
    let (facts, candidates) = dependency_facts(&input(
        "pyproject.toml",
        ContentKind::Toml,
        "[project]\ndependencies = [\"httpx\"]\n\n[project.optional-dependencies]\ndev = [\"pytest\", \"black\"]\n",
    ));

    let extras: Vec<_> = facts
        .iter()
        .filter(|fact| fact.fact_kind == "manifest_extra")
        .map(|fact| fact.name.as_str())
        .collect();
    assert_eq!(extras, vec!["dev"]);
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.kind == "manifest_extra")
    );
}

#[test]
fn extracts_toolchain_facts_across_manifest_ecosystems() {
    let samples = [
        (
            "Cargo.toml",
            ContentKind::Toml,
            "[package]\nedition = \"2021\"\nrust-version = \"1.75\"\n",
            vec![
                ("toolchain", "2021".to_string()),
                ("toolchain_version", "1.75".to_string()),
            ],
        ),
        (
            "go.mod",
            ContentKind::PlainText,
            "module example.com/axon\n\ngo 1.22\ntoolchain go1.22.3\n",
            vec![
                ("toolchain_version", "1.22".to_string()),
                ("toolchain_version", "1.22.3".to_string()),
            ],
        ),
        (
            "package.json",
            ContentKind::Json,
            r#"{"engines":{"node":">=18"}}"#,
            vec![("toolchain_version", ">=18".to_string())],
        ),
        (
            "pyproject.toml",
            ContentKind::Toml,
            "[project]\nrequires-python = \">=3.11\"\n",
            vec![("toolchain_version", ">=3.11".to_string())],
        ),
        (
            "pom.xml",
            ContentKind::Xml,
            "<project><properties><maven.compiler.release>21</maven.compiler.release></properties></project>",
            vec![("toolchain_version", "21".to_string())],
        ),
    ];

    for (path, kind, text, expected) in samples {
        let (facts, candidates) = dependency_facts(&input(path, kind, text));
        let toolchain_facts: Vec<_> = facts
            .iter()
            .filter(|fact| fact.fact_kind == "toolchain" || fact.fact_kind == "toolchain_version")
            .collect();
        assert_eq!(
            toolchain_facts.len(),
            expected.len(),
            "unexpected toolchain fact count for {path}"
        );
        for (fact, (kind, version)) in toolchain_facts.iter().zip(expected.iter()) {
            assert_eq!(fact.fact_kind, *kind, "fact_kind mismatch for {path}");
            assert_eq!(
                fact.value.get("version").and_then(|v| v.as_str()),
                Some(version.as_str()),
                "version mismatch for {path}"
            );
        }
        assert!(
            candidates.iter().any(|candidate| {
                candidate.kind == "toolchain" || candidate.kind == "toolchain_version"
            }),
            "missing toolchain graph candidate for {path}"
        );
    }
}

#[test]
fn malformed_package_json_degrades_with_warning() {
    let result = dependency_parse_result(&input(
        "package.json",
        ContentKind::Json,
        r#"{"dependencies":{"react":"19"}"#,
    ));

    assert_eq!(result.header.status, LifecycleStatus::CompletedDegraded);
    assert!(result.facts.is_empty());
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(result.warnings[0].code, "parse.manifest.invalid");
}

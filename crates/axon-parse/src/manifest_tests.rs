use axon_api::source::*;
use uuid::Uuid;

use crate::manifest::dependency_facts;
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

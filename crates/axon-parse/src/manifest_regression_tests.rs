use super::*;
use crate::parser::ParseInput;
use axon_api::source::*;

fn input(path: &str, kind: ContentKind, text: &str) -> ParseInput {
    ParseInput {
        job_id: JobId::new(uuid::Uuid::from_u128(11)),
        stage_id: StageId::new(uuid::Uuid::from_u128(12)),
        requested_parser: None,
        document: SourceDocument {
            document_id: DocumentId::from("manifest-regression"),
            source_id: SourceId::from("manifest-source"),
            source_item_key: SourceItemKey::from(path),
            canonical_uri: format!("file:///repo/{path}"),
            content_kind: kind,
            content: ContentRef::InlineText {
                text: text.to_owned(),
            },
            metadata: MetadataMap::new(),
            title: None,
            language: None,
            path: Some(path.to_owned()),
            mime_type: None,
            structured_payload: None,
            artifact_id: None,
            chunk_hints: Vec::new(),
            parser_hints: Vec::new(),
        },
    }
}

fn assert_source_evidence(text: &str, candidates: &[GraphCandidate]) {
    assert!(!candidates.is_empty());
    for candidate in candidates {
        assert!(!candidate.evidence.is_empty());
        for evidence in &candidate.evidence {
            let range = evidence.range.as_ref().expect("evidence range");
            let start = range.line_start.unwrap() as usize;
            let end = range.line_end.unwrap() as usize;
            assert!(start > 0 && end >= start);
            let excerpt = text
                .lines()
                .skip(start - 1)
                .take(end - start + 1)
                .collect::<Vec<_>>()
                .join("\n");
            let quote = evidence.quote.as_deref().expect("source quote");
            assert!(!quote.is_empty());
            assert!(
                excerpt.contains(quote),
                "quote {quote:?} outside {start}..{end}: {excerpt:?}"
            );
        }
    }
}

#[test]
fn maven_multiline_and_spaced_xml_quotes_preserve_source_lines() {
    let text = "<project>\r\n  <dependencies>\r\n    <dependency>\r\n      <groupId> org.example </groupId>\r\n      <artifactId>\r\n        café-library\r\n      </artifactId>\r\n      <version> 1.2 </version>\r\n    </dependency>\r\n  </dependencies>\r\n  <properties>\r\n    <maven.compiler.release>\r\n      21\r\n    </maven.compiler.release>\r\n  </properties>\r\n</project>\r\n";
    let parsed = dependency_parse_items(&input("pom.xml", ContentKind::Xml, text));
    assert_eq!(parsed.facts.len(), 2);
    assert_eq!(parsed.facts[0].name, "org.example:café-library");
    assert_eq!(parsed.facts[0].value["version"], "1.2");
    assert_eq!(parsed.facts[1].name, "java");
    assert_eq!(parsed.facts[1].value["version"], "21");
    assert_source_evidence(text, &parsed.graph_candidates);
    let quote = parsed.graph_candidates[0].evidence[0].quote.as_deref();
    assert_eq!(quote, Some("café-library"));
    assert_eq!(
        parsed.graph_candidates[0].evidence[0]
            .range
            .as_ref()
            .unwrap()
            .line_start,
        Some(6)
    );
}

#[test]
fn maven_single_line_toolchain_keeps_original_tag_spacing() {
    let text =
        "<project>\n<properties><java.version>  17  </java.version></properties>\n</project>";
    let parsed = dependency_parse_items(&input("pom.xml", ContentKind::Xml, text));
    assert_eq!(parsed.facts.len(), 1);
    assert_eq!(parsed.facts[0].value["version"], "17");
    assert_source_evidence(text, &parsed.graph_candidates);
}

#[test]
fn yaml_quoted_kinds_and_inferred_chart_use_existing_source_evidence() {
    let text = "# deployment\napiVersion: apps/v1\nkind:   \"Deployment\"\nmetadata:\n  name: demo\n---\n# chart\napiVersion: v2\nname: demo-chart\n---\napiVersion: custom/v1\nkind: 'Service'\nmetadata:\n  name: demo-service\n";
    let parsed = dependency_parse_items(&input("deploy.yaml", ContentKind::Yaml, text));
    assert_eq!(
        parsed
            .facts
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        [
            "Deployment/demo",
            "Chart/demo-chart",
            "Service/demo-service"
        ]
    );
    assert_source_evidence(text, &parsed.graph_candidates);
    let chart = &parsed.graph_candidates[1].evidence[0];
    assert_eq!(chart.quote.as_deref(), Some("apiVersion: v2"));
    assert_eq!(chart.range.as_ref().unwrap().line_start, Some(8));
}

use super::*;
use crate::text::source_range;
use axon_api::source::{DocumentId, MetadataMap, SourceItemKey};

fn fact(text: &str, name: &str, start: usize, end: usize) -> SourceParseFacts {
    SourceParseFacts {
        document_id: DocumentId::from("doc"),
        source_item_key: SourceItemKey::from("file"),
        fact_kind: "code_symbol".into(),
        name: name.into(),
        value: serde_json::json!({"symbol_kind": "function"}),
        parser_id: "code_symbols".into(),
        parser_version: "test".into(),
        parser_method: "tree_sitter".into(),
        range: Some(source_range(text, start, end)),
        confidence: 0.95,
        metadata: MetadataMap::new(),
    }
}

#[test]
fn nested_symbols_partition_source_once_with_utf8_and_module_context() {
    for text in [
        "// café\nimpl Widget {\n fn first() {}\n fn second() {}\n}\nuse x;\n",
        "# café\nclass Widget:\n def first(self):\n  pass\n def second(self):\n  pass\nx = 1\n",
        "// café\nfunction outer() { function inner() {} return 1; }\nrun();\n",
    ] {
        let start = text
            .find("impl")
            .or_else(|| text.find("class"))
            .or_else(|| text.find("function outer"))
            .unwrap();
        let child_start = text
            .find("fn first")
            .or_else(|| text.find("def first"))
            .or_else(|| text.find("function inner"))
            .unwrap();
        let (child_end, container_end) = if text.contains("impl") {
            (
                child_start + "fn first() {}".len(),
                text.find("\nuse x").unwrap(),
            )
        } else if text.contains("class") {
            (
                text.find("\n def second").unwrap(),
                text.find("\nx = 1").unwrap(),
            )
        } else {
            (
                child_start + "function inner() {}".len(),
                text.find("\nrun()").unwrap(),
            )
        };
        let facts = vec![
            fact(text, "container", start, container_end),
            fact(text, "child", child_start, child_end),
            fact(text, "duplicate", child_start, child_end),
        ];
        let chunks = parser_code_symbol_chunks(text, &facts).unwrap();
        assert_eq!(
            chunks
                .iter()
                .map(|c| c.content.as_str())
                .collect::<String>(),
            text
        );
        let mut previous_end = 0;
        for chunk in &chunks {
            let left = chunk.range.byte_start.unwrap() as usize;
            let right = chunk.range.byte_end.unwrap() as usize;
            assert_eq!(left, previous_end);
            assert_eq!(chunk.content, text[left..right]);
            previous_end = right;
        }
        assert_eq!(previous_end, text.len());
        let child = chunks
            .iter()
            .find(|c| c.symbol.as_deref() == Some("child"))
            .unwrap();
        assert_eq!(
            child.metadata["code_symbol_aliases"],
            serde_json::json!(["child", "duplicate"])
        );
        assert_eq!(
            chunks
                .iter()
                .filter(|c| c.symbol.as_deref() == Some("child"))
                .count(),
            1
        );
        assert!(
            chunks
                .iter()
                .any(|c| c.symbol.as_deref() == Some("container"))
        );
    }
}

#[test]
fn thousands_of_symbol_intervals_avoid_repeated_source_prefix_scans() {
    let source = "fn render() { let name = \"café\"; }\n".repeat(2000);
    let facts = source
        .split_inclusive('\n')
        .scan(0, |start, line| {
            let end = *start + line.len() - 1;
            let result = fact(&source, "render", *start, end);
            *start += line.len();
            Some(result)
        })
        .collect::<Vec<_>>();
    let (chunks, work) = crate::performance_measurement::measure(|| {
        parser_code_symbol_chunks(&source, &facts).unwrap()
    });
    assert_eq!(
        chunks
            .iter()
            .map(|c| c.content.as_str())
            .collect::<String>(),
        source
    );
    assert!(
        work.range_scan_bytes <= source.len() * 4,
        "repeated prefix scans processed {} bytes for {} source bytes",
        work.range_scan_bytes,
        source.len()
    );
    assert_eq!(
        chunks.iter().filter(|c| c.symbol.is_some()).count(),
        facts.len()
    );
}

#[test]
fn module_remainder_uses_the_parser_method_and_truthful_recovery_status() {
    let source = "use crate::Widget;\nfn run() {}\n";
    let start = source.find("fn run").unwrap();
    let mut symbol = fact(source, "run", start, source.len() - 1);
    for recovered in [false, true] {
        symbol.value["code_syntax_recovered"] = recovered.into();
        symbol.value["code_parse_status"] = if recovered { "partial" } else { "parsed" }.into();
        let chunks = parser_code_symbol_chunks(source, &[symbol.clone()]).unwrap();
        assert_eq!(chunks[0].metadata["actual_chunking_method"], "tree_sitter");
        assert_eq!(chunks[0].metadata["code_syntax_recovered"], recovered);
        assert_eq!(
            chunks[0].metadata["code_parse_status"],
            if recovered { "partial" } else { "parsed" }
        );
    }
}

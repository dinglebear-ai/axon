use super::CharOffsets;

#[test]
fn sparse_offsets_preserve_unicode_with_bounded_lookup_scans() {
    let source = "fn café() { let 界 = \"🙂\"; }\r\n".repeat(4000);
    let offsets = CharOffsets::new(&source);
    assert!(offsets.checkpoints.len() <= source.len() / 256 + 1);
    for (chars, (byte, _)) in source.char_indices().enumerate() {
        assert_eq!(offsets.at(byte), chars as u64);
        let index = offsets
            .checkpoints
            .partition_point(|&(offset, _)| offset <= byte)
            - 1;
        assert!(byte - offsets.checkpoints[index].0 < 260);
    }
    assert_eq!(offsets.at(source.len()), source.chars().count() as u64);
}

#[test]
fn elixir_graph_identity_distinguishes_arity_and_module_but_merges_clauses() {
    let mut input = crate::code::tests::input(
        "modules.ex",
        "defmodule Outer do\n defmodule Inner do\n  def foo(value), do: value\n  def foo(nil), do: 0\n  def foo(left, right), do: {left, right}\n end\n def foo(value), do: value\nend\n",
    );
    let (facts, graph) = crate::code::symbol_facts_with_graph(&input);
    assert_eq!(facts.len(), 6);
    assert_eq!(graph[2].nodes[1].label, "Outer.Inner.foo/1");
    assert_eq!(graph[4].nodes[1].label, "Outer.Inner.foo/2");
    assert_eq!(graph[5].nodes[1].label, "Outer.foo/1");
    assert_eq!(graph[2].candidate_id, graph[3].candidate_id);
    assert_eq!(graph[2].merge_key, graph[3].merge_key);
    assert_ne!(
        graph[2].evidence[0].evidence_id,
        graph[3].evidence[0].evidence_id
    );
    assert_ne!(graph[2].nodes[1].stable_key, graph[4].nodes[1].stable_key);
    assert_ne!(graph[2].candidate_id, graph[5].candidate_id);
    assert_eq!(graph[2].nodes[1].properties["symbol_arity"], 1);
    assert_eq!(facts[2].value["parent_symbol"], "Outer.Inner");
    input.document.source_id = axon_api::source::SourceId::from("different-source");
    let (_, other_graph) = crate::code::symbol_facts_with_graph(&input);
    assert_ne!(graph[2].candidate_id, other_graph[2].candidate_id);
}

#[test]
fn tsx_outcome_names_the_actual_grammar_without_changing_symbol_language() {
    let input = crate::code::tests::input("view.tsx", "export function View() { return <div />; }");
    let (facts, _, _) = crate::code::parse_with_outcome(&input);
    assert_eq!(facts[0].value["language"], "typescript");
    assert_eq!(facts.last().unwrap().value["code_grammar"], "tsx");
}

#[test]
fn elixir_protocol_implementation_graph_scopes_include_the_type() {
    let input = crate::code::tests::input(
        "implementations.ex",
        "defimpl Demo.Protocol, for: First do\n def run(value), do: value\nend\ndefimpl Demo.Protocol, for: Second do\n def run(value), do: value\nend\n",
    );
    let (facts, graph) = crate::code::symbol_facts_with_graph(&input);
    assert_eq!(facts.len(), 4);
    assert_eq!(facts[0].name, "Demo.Protocol for First");
    assert_eq!(facts[0].value["symbol_kind"], "impl");
    assert_eq!(graph[1].nodes[1].label, "Demo.Protocol for First.run/1");
    assert_ne!(graph[0].candidate_id, graph[2].candidate_id);
    assert_ne!(graph[1].candidate_id, graph[3].candidate_id);
}

#[test]
fn recovered_elixir_functions_keep_clean_module_head_scope() {
    let input = crate::code::tests::input(
        "recovered.ex",
        "defmodule First do\n def good(), do: :ok\n bad = )\nend\ndefmodule Second do\n def good(), do: :ok\n broken = )\nend\n",
    );
    let (facts, graph) = crate::code::symbol_facts_with_graph(&input);
    assert_eq!(facts.len(), 2, "{facts:?}");
    assert_eq!(facts[0].value["parent_symbol"], "First");
    assert_eq!(facts[1].value["parent_symbol"], "Second");
    assert_eq!(facts[0].value["code_parse_status"], "partial");
    assert_eq!(graph[0].nodes[1].label, "First.good/0");
    assert_eq!(graph[1].nodes[1].label, "Second.good/0");
    assert_ne!(graph[0].candidate_id, graph[1].candidate_id);
}

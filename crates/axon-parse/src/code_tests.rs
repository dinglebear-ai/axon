use axon_api::source::*;
use uuid::Uuid;

use crate::code::{symbol_facts, symbol_facts_with_graph};
use crate::parser::ParseInput;

pub(super) fn input(path: &str, text: &str) -> ParseInput {
    ParseInput {
        job_id: JobId::new(Uuid::from_u128(1)),
        stage_id: StageId::new(Uuid::from_u128(2)),
        requested_parser: None,
        document: SourceDocument {
            document_id: DocumentId::from("doc_code"),
            source_id: SourceId::from("src_repo"),
            source_item_key: SourceItemKey::from(path),
            canonical_uri: format!("file:///repo/{path}"),
            content_kind: ContentKind::Code,
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
fn extracts_simple_rust_symbol_facts() {
    let facts = symbol_facts(&input(
        "src/lib.rs",
        "pub struct Parser;\nfn parse_one() {}\n",
    ));

    let names: Vec<_> = facts.iter().map(|fact| fact.name.as_str()).collect();
    assert_eq!(names, vec!["Parser", "parse_one"]);
    assert!(facts.iter().all(|fact| fact.fact_kind == "code_symbol"));
    assert_eq!(facts[0].value["symbol_kind"], "struct");
    assert_eq!(facts[1].value["language"], "rust");
}

#[test]
fn emits_graph_candidates_for_code_symbols_without_breaking_fact_api() {
    let input = input("src/lib.rs", "pub enum Mode {}\nasync fn run() {}\n");
    let fact_only = symbol_facts(&input);
    let (facts, candidates) = symbol_facts_with_graph(&input);

    assert_eq!(facts, fact_only);
    assert_eq!(candidates.len(), facts.len());
    assert!(candidates.iter().all(|candidate| {
        candidate.kind == "code_symbol"
            && !candidate.nodes.is_empty()
            && !candidate.evidence.is_empty()
    }));
    assert_eq!(
        candidates[0].producer.parser.as_deref(),
        Some("code_symbols")
    );
    assert_eq!(
        candidates[0].evidence[0].quote.as_deref(),
        Some("pub enum Mode {}")
    );
}

#[test]
fn ignores_commented_out_symbols() {
    let facts = symbol_facts(&input(
        "src/lib.rs",
        "// fn not_real() {}\n# def not_python():\n/* struct NotReal; */\nfn real() {}\n",
    ));

    let names: Vec<_> = facts.iter().map(|fact| fact.name.as_str()).collect();
    assert_eq!(names, vec!["real"]);
}

#[test]
fn supported_rust_is_tree_sitter_backed() {
    let facts = symbol_facts(&input("src/lib.rs", "pub fn run() {}\n"));

    assert_eq!(facts[0].parser_method, "tree_sitter");
    assert!(facts[0].confidence >= 0.9);
    assert_eq!(facts[0].value["symbol_extraction_status"], "ast");
}

#[test]
fn rust_visibility_is_derived_from_the_pub_keyword() {
    let facts = symbol_facts(&input(
        "src/lib.rs",
        "pub struct Public;\nstruct Private;\n",
    ));

    assert_eq!(facts[0].value["symbol_visibility"], "public");
    assert_eq!(facts[1].value["symbol_visibility"], "private");
}

#[test]
fn python_visibility_is_derived_from_a_leading_underscore() {
    let facts = symbol_facts(&input(
        "pkg/mod.py",
        "def public_fn():\n    pass\ndef _private_fn():\n    pass\n",
    ));

    assert_eq!(facts[0].value["symbol_visibility"], "public");
    assert_eq!(facts[1].value["symbol_visibility"], "private");
}

#[test]
fn nested_python_method_records_its_class_as_parent_symbol() {
    let facts = symbol_facts(&input(
        "pkg/mod.py",
        "class Widget:\n    def render(self):\n        pass\n",
    ));

    assert_eq!(facts[0].name, "Widget");
    assert!(facts[0].value["parent_symbol"].is_null());
    assert_eq!(facts[1].name, "render");
    assert_eq!(facts[1].value["parent_symbol"], "Widget");
}

#[test]
fn rust_function_span_covers_its_full_brace_body() {
    let source = "pub fn run() {\n    let x = 1;\n    println!(\"{x}\");\n}\n";
    let facts = symbol_facts(&input("src/lib.rs", source));

    let range = facts[0].range.as_ref().expect("range");
    assert_eq!(range.line_start, Some(1));
    assert_eq!(range.line_end, Some(4));
    assert_eq!(range.byte_start, Some(0));
    assert_eq!(range.byte_end, Some(source.trim_end().len() as u64));
}

#[test]
fn rust_ast_range_ignores_braces_inside_strings() {
    let source = "fn render() {\n    let close = \"}\";\n    let value = 1;\n}\nfn next() {}\n";
    let facts = symbol_facts(&input("src/lib.rs", source));

    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].range.as_ref().unwrap().line_end, Some(4));
    assert_eq!(
        facts[0].range.as_ref().unwrap().byte_end,
        Some(source.find("\nfn next").unwrap() as u64)
    );
}

#[test]
fn rust_unit_struct_span_is_single_line() {
    let facts = symbol_facts(&input("src/lib.rs", "pub struct Parser;\n"));

    let range = facts[0].range.as_ref().expect("range");
    assert_eq!(range.line_start, Some(1));
    assert_eq!(range.line_end, Some(1));
}

#[test]
fn python_function_span_covers_its_indented_body() {
    let facts = symbol_facts(&input(
        "pkg/mod.py",
        "def run():\n    a = 1\n    b = 2\n\nc = 3\n",
    ));

    let range = facts[0].range.as_ref().expect("range");
    assert_eq!(range.line_start, Some(1));
    assert_eq!(range.line_end, Some(3));
}

#[test]
fn every_symbol_range_is_ordered_and_therefore_survives_sanitization() {
    let (facts, candidates) = symbol_facts_with_graph(&input(
        "src/lib.rs",
        "pub struct Parser;\nasync fn run() {\n    let _ = 1;\n}\nclass PyThing:\n    def run(self):\n        pass\n",
    ));

    for fact in &facts {
        let range = fact.range.as_ref().expect("every code_symbol has a range");
        assert!(range.line_start.unwrap() <= range.line_end.unwrap());
    }
    assert_eq!(facts.len(), candidates.len());
}

#[test]
fn extracts_typescript_symbol_facts_at_heuristic_parity() {
    let facts = symbol_facts(&input(
        "src/component.tsx",
        "export interface Props {\n  name: string\n}\n\
export type Mode = 'light' | 'dark';\n\
export enum Size { Small, Large }\n\
export class Widget {\n  render() { return null; }\n}\n\
export const useWidget = (name: string) => ({ name });\n",
    ));

    let names: Vec<_> = facts.iter().map(|fact| fact.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Props", "Mode", "Size", "Widget", "render", "useWidget"]
    );
    assert!(
        facts
            .iter()
            .all(|fact| fact.value["language"] == "typescript")
    );
    assert_eq!(facts[0].value["symbol_kind"], "interface");
    assert_eq!(facts[1].value["symbol_kind"], "type");
    assert_eq!(facts[2].value["symbol_kind"], "enum");
    assert_eq!(facts[3].value["symbol_kind"], "class");
    assert_eq!(facts[4].value["symbol_kind"], "method");
    assert_eq!(facts[4].value["parent_symbol"], "Widget");
    assert_eq!(facts[5].value["symbol_kind"], "function");
    assert_eq!(facts[5].value["symbol_visibility"], "public");
    assert!(facts.iter().all(|fact| fact.parser_method == "tree_sitter"));
}

#[test]
fn extracts_javascript_function_class_and_assignment_symbols() {
    let facts = symbol_facts(&input(
        "src/widget.js",
        "export default function createWidget() {\n  return {}\n}\n\
class LocalWidget {}\n\
const helper = function () { return 1; }\n\
let arrow = () => 2;\n\
var value = 3;\n",
    ));

    let pairs: Vec<_> = facts
        .iter()
        .map(|fact| {
            (
                fact.name.as_str(),
                fact.value["symbol_kind"].as_str().unwrap(),
                fact.value["symbol_visibility"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("createWidget", "function", "public"),
            ("LocalWidget", "class", "private"),
            ("helper", "function", "private"),
            ("arrow", "function", "private"),
            ("value", "constant", "private"),
        ]
    );
    assert!(
        facts
            .iter()
            .all(|fact| fact.value["language"] == "javascript")
    );
    assert!(facts.iter().all(|fact| fact.parser_method == "tree_sitter"));
}

#[test]
fn supported_python_is_tree_sitter_backed_with_nested_parent_ranges() {
    let source = "class Widget:\n    def render(self):\n        return 1\n";
    let facts = symbol_facts(&input("pkg/widget.py", source));

    assert_eq!(facts.len(), 2);
    assert!(facts.iter().all(|fact| fact.parser_method == "tree_sitter"));
    assert_eq!(facts[1].value["parent_symbol"], "Widget");
    let method = facts[1].range.as_ref().unwrap();
    assert_eq!(method.line_start, Some(2));
    assert_eq!(method.line_end, Some(3));
    assert_eq!(method.byte_start, Some(18));
    assert_eq!(method.byte_end, Some(source.trim_end().len() as u64));
}

#[test]
fn malformed_supported_declaration_does_not_become_a_graph_symbol() {
    let (facts, candidates) = symbol_facts_with_graph(&input("src/lib.rs", "fn broken() {\n"));
    assert!(facts.is_empty());
    assert!(candidates.is_empty());
}

#[test]
fn unsupported_language_keeps_honest_regex_fallback() {
    let facts = symbol_facts(&input("src/main.rb", "class Widget\nend\n"));

    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].parser_method, "regex_fallback");
    assert!(facts[0].confidence < 0.75);
}

#[test]
fn syntax_recovery_retains_valid_symbols_without_claiming_malformed_ones() {
    let text = "fn before() {}\nfn broken() { let x = ; }\nfn after() {}\n";
    let (facts, candidates) = symbol_facts_with_graph(&input("src/lib.rs", text));
    let names: Vec<_> = facts.iter().map(|fact| fact.name.as_str()).collect();
    assert_eq!(names, ["before", "after"]);
    assert_eq!(candidates.len(), facts.len());
    for fact in facts {
        assert_eq!(fact.parser_method, "tree_sitter");
        assert_eq!(fact.value["code_parse_status"], "partial");
        assert_eq!(fact.value["code_syntax_recovered"], true);
    }
}

#[test]
fn nested_ast_symbols_keep_full_graph_facts_and_unicode_ranges() {
    for (path, text, parent, child) in [
        (
            "src/lib.rs",
            "// café\nimpl Widget { fn render() {} }\n",
            "Widget",
            "render",
        ),
        (
            "src/widget.ts",
            "// café\nclass Widget { render() {} }\n",
            "Widget",
            "render",
        ),
    ] {
        let (facts, candidates) = symbol_facts_with_graph(&input(path, text));
        assert_eq!(facts.len(), 2);
        assert_eq!(candidates.len(), 2);
        assert_eq!(facts[0].name, parent);
        assert_eq!(facts[1].name, child);
        assert_eq!(facts[1].value["parent_symbol"], parent);
        for fact in &facts {
            let range = fact.range.as_ref().unwrap();
            let start = range.byte_start.unwrap() as usize;
            let end = range.byte_end.unwrap() as usize;
            assert_eq!(range.char_start, Some(text[..start].chars().count() as u64));
            assert_eq!(range.char_end, Some(text[..end].chars().count() as u64));
            assert!(text[start..end].contains(&fact.name));
        }
    }
}

#[test]
fn shared_declaration_ranges_keep_both_variable_graph_facts() {
    let (facts, candidates) = symbol_facts_with_graph(&input(
        "src/config.ts",
        "export const first = 1, second = 2;\n",
    ));
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].name, "first");
    assert_eq!(facts[1].name, "second");
    assert_eq!(facts[0].range, facts[1].range);
    assert_eq!(candidates.len(), 2);
}

#[test]
fn extracts_bash_functions_with_exact_unicode_ranges() {
    let text = "#!/bin/bash\n# café\nexport MODE=ready\nfunction launch() { printf '%s' \"🙂\"; }\nprintf done\n";
    let facts = symbol_facts(&input("run.sh", text));
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].name, "launch");
    assert_eq!(facts[0].parser_method, "tree_sitter");
    let range = facts[0].range.as_ref().unwrap();
    let start = range.byte_start.unwrap() as usize;
    let end = range.byte_end.unwrap() as usize;
    assert_eq!(
        &text[start..end],
        "function launch() { printf '%s' \"🙂\"; }"
    );
    assert_eq!(
        range.char_start.unwrap(),
        text[..start].chars().count() as u64
    );
}

#[test]
fn extracts_nested_css_rules_and_at_rules() {
    let facts = symbol_facts(&input(
        "theme.css",
        "@media (min-width: 600px) { .card:hover, #main { color: red; } }\n@keyframes spin { to { transform: rotate(360deg); } }",
    ));
    assert!(facts.iter().any(|f| f.name == "@media (min-width: 600px)"));
    let rule = facts
        .iter()
        .find(|f| f.name == ".card:hover, #main")
        .unwrap();
    assert_eq!(rule.value["parent_symbol"], "@media (min-width: 600px)");
    assert!(facts.iter().any(|f| f.name == "@keyframes spin"));
}

#[test]
fn extracts_elixir_visibility_guards_macros_and_clauses() {
    let text = "defmodule Demo do\n @doc \"Runs\"\n @spec run(integer()) :: integer()\n def run(value) when is_integer(value), do: value\n def run(nil), do: 0\n def clauses(value)\n def clauses(value), do: value\n defp secret(), do: :ok\n defmacro wrap(value), do: value\n defmacrop hidden(value), do: value\nend\n";
    let facts = symbol_facts(&input("demo.ex", text));
    assert_eq!(facts.len(), 8);
    assert_eq!(facts.iter().filter(|f| f.name == "run").count(), 2);
    let run = facts.iter().find(|f| f.name == "run").unwrap();
    assert_eq!(run.value["symbol_arity"], 1);
    assert_eq!(run.value["parent_symbol"], "Demo");
    assert_eq!(
        facts.iter().find(|f| f.name == "secret").unwrap().value["symbol_visibility"],
        "private"
    );
    let hidden = facts.iter().find(|f| f.name == "hidden").unwrap();
    assert_eq!(hidden.value["symbol_kind"], "macro");
    assert_eq!(hidden.value["symbol_visibility"], "private");
    assert!(facts.iter().all(|f| f.parser_method == "tree_sitter"));
}

#[test]
fn explicit_outcomes_distinguish_zero_symbols_and_unsupported_dialects() {
    for (path, text, expected) in [
        ("a.sh", "echo ready\n", "parsed"),
        ("a.css", "/* empty stylesheet */", "parsed"),
        ("a.exs", "IO.puts(\"ready\")", "parsed"),
        ("a.zsh", "function run() { echo hi; }", "unsupported"),
        ("a.scss", ".card { color: $red; }", "unsupported"),
        ("a.heex", "<%= @value %>", "unsupported"),
    ] {
        let (facts, _, _) = crate::code::parse_with_outcome(&input(path, text));
        let outcome = facts
            .iter()
            .find(|f| f.fact_kind == "code_parse_outcome")
            .unwrap();
        assert_eq!(outcome.value["code_ast_status"], expected, "{path}");
        assert_eq!(outcome.value["code_symbol_count"], 0);
    }
}

#[test]
fn new_grammars_disclose_recovery_without_inventing_malformed_symbols() {
    for (path, text, good, bad) in [
        ("run.sh", "good() { echo ready; }\nbad() {\n", "good", "bad"),
        ("demo.ex", "def good(), do: :ok\ndef bad(\n", "good", "bad"),
        (
            "theme.css",
            ".good { color: red; }\n.bad { color:\n",
            ".good",
            ".bad",
        ),
    ] {
        let (facts, _, _) = crate::code::parse_with_outcome(&input(path, text));
        let outcome = facts
            .iter()
            .find(|f| f.fact_kind == "code_parse_outcome")
            .unwrap();
        assert_eq!(outcome.value["code_ast_status"], "partial", "{path}");
        assert!(
            facts
                .iter()
                .any(|f| f.fact_kind == "code_symbol" && f.name == good),
            "{path}: {facts:?}"
        );
        assert!(
            !facts
                .iter()
                .any(|f| f.fact_kind == "code_symbol" && f.name == bad),
            "{path}"
        );
    }
}

#[test]
fn actual_repository_shell_and_css_files_use_structural_grammars() {
    for (path, text, grammar) in [
        ("install.sh", include_str!("../../../install.sh"), "bash"),
        (
            "apps/palette-tauri/src/styles.css",
            include_str!("../../../apps/palette-tauri/src/styles.css"),
            "css",
        ),
    ] {
        let (facts, graph, fallback) = crate::code::parse_with_outcome(&input(path, text));
        assert!(!fallback, "{path}");
        assert!(!graph.is_empty(), "{path}");
        let outcome = facts
            .iter()
            .find(|f| f.fact_kind == "code_parse_outcome")
            .unwrap();
        assert_eq!(outcome.value["code_grammar"], grammar);
        for fact in facts.iter().filter(|f| f.fact_kind == "code_symbol") {
            let range = fact.range.as_ref().unwrap();
            let quote = &text[range.byte_start.unwrap() as usize..range.byte_end.unwrap() as usize];
            assert!(!quote.is_empty());
        }
    }
}

#[test]
fn elixir_attributes_are_retained_and_quoted_defs_are_not_declarations() {
    let text = "@doc \"Runs\"\n@spec run(integer()) :: integer()\ndef run(value), do: value\nquote do\n def invented(), do: :ok\nend\n";
    let facts = symbol_facts(&input("demo.exs", text));
    assert_eq!(facts.len(), 1);
    let range = facts[0].range.as_ref().unwrap();
    assert_eq!(range.byte_start, Some(0));
    assert_eq!(range.line_start, Some(1));
    assert_eq!(facts[0].name, "run");
}

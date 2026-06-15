use super::super::{ChunkSource, SymbolKind, chunk_code_chunks};

// ── leaf declaration → exactly one chunk ──────────────────────────────

#[test]
fn single_leaf_is_one_chunk() {
    let src = "fn hello() {\n    println!(\"hi\");\n}\n";
    let chunks = chunk_code_chunks(src, "rs").unwrap();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].symbol_name(), Some("hello"));
    assert_eq!(chunks[0].symbol_kind(), Some(SymbolKind::Function));
    assert!(chunks[0].text.contains("fn hello"));
}

// ── impl with 2 methods → 2 method chunks, body not duplicated ────────

#[test]
fn impl_two_methods_emits_two_method_chunks() {
    let src = "impl T {\n    fn a(&self) {\n        let _ = 1;\n    }\n    fn b(&self) {\n        let _ = 2;\n    }\n}\n";
    let chunks = chunk_code_chunks(src, "rs").unwrap();

    let method_a = chunks
        .iter()
        .filter(|c| c.symbol_name().is_some_and(|n| n.ends_with("a")))
        .count();
    let method_b = chunks
        .iter()
        .filter(|c| c.symbol_name().is_some_and(|n| n.ends_with("b")))
        .count();
    assert_eq!(method_a, 1, "method a should emit exactly once");
    assert_eq!(method_b, 1, "method b should emit exactly once");

    // The container body must not be re-emitted as a full chunk: no single chunk
    // should contain BOTH method bodies.
    assert!(
        !chunks
            .iter()
            .any(|c| c.text.contains("let _ = 1") && c.text.contains("let _ = 2")),
        "container body should not be emitted as one chunk holding both methods"
    );
}

// ── nested fn inside a fn → only the parent emits ─────────────────────

#[test]
fn nested_fn_folds_into_parent() {
    let src = "fn outer() {\n    fn inner() {\n        let _ = 1;\n    }\n    inner();\n}\n";
    let chunks = chunk_code_chunks(src, "rs").unwrap();
    let names: Vec<_> = chunks.iter().filter_map(|c| c.symbol_name()).collect();
    assert!(names.contains(&"outer"), "outer must emit");
    assert!(
        !names.contains(&"inner"),
        "nested inner fn must fold into parent, got {names:?}"
    );
}

// ── oversized leaf → multiple sub-chunks, each ≤ cap, each carries symbol ─

#[test]
fn oversized_leaf_splits_into_bounded_subchunks() {
    let mut src = String::from("fn big() {\n");
    for i in 0..200 {
        src.push_str(&format!("    let var_{i} = {i} * 2 + 1;\n"));
    }
    src.push_str("}\n");
    assert!(src.len() > 2000);

    let chunks = chunk_code_chunks(&src, "rs").unwrap();
    let big_chunks: Vec<_> = chunks
        .iter()
        .filter(|c| c.symbol_name() == Some("big"))
        .collect();
    assert!(
        big_chunks.len() > 1,
        "oversized fn should split, got {}",
        big_chunks.len()
    );
    for c in &big_chunks {
        assert!(
            c.text.chars().count() <= 2000,
            "sub-chunk over cap: {} chars",
            c.text.chars().count()
        );
        assert_eq!(c.symbol_kind(), Some(SymbolKind::Function));
    }
}

// ── residual run of `}` / `;` / blanks → dropped (no sliver) ──────────

#[test]
fn punctuation_residual_dropped() {
    // Two functions separated by blank lines and stray braces — the gap is all
    // punctuation and must not become a chunk.
    let src = "fn a() {\n    let _ = 1;\n}\n\n;\n}\n\nfn b() {\n    let _ = 2;\n}\n";
    let chunks = chunk_code_chunks(src, "rs").unwrap();
    assert!(
        chunks.iter().all(|c| c.symbol.is_some()),
        "no symbol-less sliver should survive: {:?}",
        chunks
            .iter()
            .filter(|c| c.symbol.is_none())
            .map(|c| &c.text)
            .collect::<Vec<_>>()
    );
}

// ── real top-level code above the floor → kept as prose ───────────────

#[test]
fn substantial_top_of_file_kept_as_prose() {
    // A non-declaration top-of-file run (e.g. a macro invocation block) whose
    // stripped length clears the floor must survive as a prose chunk, while the
    // surrounding `const`/`fn` leaves emit their own chunks.
    let preamble = "println!(\"a genuinely substantial first line of meaningful top-of-file content\");\nlet computed_value = compute_something_meaningful_here(1, 2, 3, 4, 5, 6, 7, 8);\ndo_more_real_setup_work_at_module_top_that_clears_the_floor_easily();\n";
    let src = format!("{preamble}\nfn after() {{\n    let _ = 1;\n}}\n");
    let chunks = chunk_code_chunks(&src, "rs").unwrap();
    assert!(
        chunks
            .iter()
            .any(|c| c.symbol.is_none() && c.text.contains("compute_something_meaningful_here")),
        "substantial residual prose must be kept"
    );
}

// ── oversized residual gap → split into bounded prose chunks ──────────

#[test]
fn oversized_residual_gap_is_split_under_cap() {
    // A large non-declaration span between captured declarations (here a big
    // top-level object-literal const, which the TS arrow rules do NOT capture)
    // must be split into multiple bounded prose chunks — never emitted as one
    // oversized chunk that blows past MAX_CODE_CHUNK_CHARS and dilutes retrieval.
    let big = (0..240)
        .map(|i| format!("  key{i}: \"value {i} with a bit of padding text to add length\","))
        .collect::<Vec<_>>()
        .join("\n");
    let src = format!(
        "export function keep(): number {{\n  return 1;\n}}\n\nconst BIG = {{\n{big}\n}};\n"
    );
    let chunks = chunk_code_chunks(&src, "ts").unwrap();

    // Every chunk (declaration or prose) stays within the cap.
    for c in &chunks {
        assert!(
            c.text.chars().count() <= 2000,
            "chunk exceeds cap ({} chars): {:?}",
            c.text.chars().count(),
            &c.text[..c.text.len().min(60)],
        );
    }
    // The big const span is too large for one chunk → it produced multiple
    // symbol-less prose chunks.
    let prose = chunks
        .iter()
        .filter(|c| c.symbol.is_none() && c.source == ChunkSource::Prose)
        .count();
    assert!(
        prose >= 2,
        "oversized residual must split into >=2 prose chunks, got {prose}: {:?}",
        chunks
            .iter()
            .map(|c| c.text.chars().count())
            .collect::<Vec<_>>(),
    );
    // The captured declaration is still present.
    assert!(chunks.iter().any(|c| c.symbol_name() == Some("keep")));
}

// ── barrel file (only re-exports) → non-empty PROSE fallback ──────────

#[test]
fn barrel_file_falls_back_to_prose() {
    let src = "pub use crate::foo::Bar;\npub use crate::baz::Qux;\npub use crate::a::B;\n";
    let chunks = chunk_code_chunks(src, "rs").unwrap();
    assert!(!chunks.is_empty(), "barrel file must not be zero chunks");
    assert!(
        chunks.iter().all(|c| c.source == ChunkSource::Prose),
        "barrel file with no declarations should be prose"
    );
}

// ── empty file → vec![] ───────────────────────────────────────────────

#[test]
fn empty_file_is_empty_vec() {
    assert!(chunk_code_chunks("", "rs").unwrap().is_empty());
    assert!(chunk_code_chunks("   \n\n  \t ", "rs").unwrap().is_empty());
}

// ── multibyte fixture → no panic, correct lines ───────────────────────

#[test]
fn multibyte_identifiers_and_literals_no_panic() {
    let src = "fn 函数() {\n    let s = \"café — dash 日本語\";\n    println!(\"{s}\");\n}\n";
    let chunks = chunk_code_chunks(src, "rs").unwrap();
    assert!(!chunks.is_empty());
    assert!(chunks.iter().all(|c| !c.text.is_empty()));
    // Line ranges must be sane (1-based, start ≤ end).
    for c in &chunks {
        assert!(c.start_line >= 1 && c.start_line <= c.end_line);
    }
}

#[test]
fn multibyte_oversized_leaf_no_panic() {
    let body = "    let _s = \"café 日本語 — \";\n".repeat(200);
    let src = format!("fn 大きい() {{\n{body}}}\n");
    assert!(src.len() > 2000);
    let chunks = chunk_code_chunks(&src, "rs").unwrap();
    assert!(!chunks.is_empty());
    assert!(chunks.iter().all(|c| c.text.chars().count() <= 2000));
}

// ── >2MB synthetic file → prose, non-empty ────────────────────────────

#[test]
fn oversized_file_routes_to_prose() {
    // Exceed the 2 MB tree-sitter ceiling: build > 2 MB of valid-ish source.
    let unit = "fn filler() {\n    let _ = 1;\n}\n";
    let reps = (2 * 1024 * 1024 / unit.len()) + 10;
    let src = unit.repeat(reps);
    assert!(src.len() > 2 * 1024 * 1024);
    let chunks = chunk_code_chunks(&src, "rs").unwrap();
    assert!(!chunks.is_empty(), "oversized file must not be zero chunks");
    assert!(
        chunks.iter().all(|c| c.source == ChunkSource::Prose),
        "oversized file must degrade to prose"
    );
}

#[test]
fn observe_sliver_rate_on_realistic_file() {
    // A realistic Rust file: impl with several methods, free fns, consts.
    let src = r#"
use std::fmt;

const MAX: usize = 100;

pub struct Engine {
    name: String,
}

impl Engine {
    pub fn new(name: String) -> Self {
        Self { name }
    }

    pub fn run(&self) -> bool {
        if self.name.is_empty() {
            return false;
        }
        let closure = || self.name.len();
        closure() > 0
    }

    fn helper(&self) -> usize {
        self.name.len()
    }
}

pub fn standalone() -> i32 {
    42
}

fn another() {
    println!("hi");
}
"#;
    let chunks = chunk_code_chunks(src, "rs").unwrap();
    // Every chunk carries a symbol; the impl emits a header (not its body), each
    // method emits once, the closure inside `run` folds into its parent.
    let slivers = chunks
        .iter()
        .filter(|c| c.symbol.is_none() && c.text.trim().chars().count() < 80)
        .count();
    assert_eq!(slivers, 0, "no tiny symbol-less slivers expected");
    assert!(
        chunks.iter().all(|c| c.symbol.is_some()),
        "every chunk in a declaration-dense file should carry a symbol"
    );
    let method_names: Vec<_> = chunks
        .iter()
        .filter(|c| c.symbol_kind() == Some(SymbolKind::Method))
        .filter_map(|c| c.symbol_name())
        .collect();
    assert!(method_names.contains(&"Engine::new"));
    assert!(method_names.contains(&"Engine::run"));
    assert!(method_names.contains(&"Engine::helper"));
}

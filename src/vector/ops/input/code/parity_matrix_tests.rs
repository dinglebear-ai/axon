//! Declaration-capture quality suite for the query-capture chunker.
//!
//! These tests assert *what symbols come out* (name + kind) and *structural
//! invariants* (no symbol-less slivers, multibyte safety, fallback behavior,
//! 1-indexed range correctness) rather than brittle raw chunk counts. The
//! headline is [`per_language_parity_matrix`] and the measurable proof of the
//! whole rework is [`anti_sliver_invariant_holds_across_languages`].

use super::{ChunkSource, CodeChunk, SymbolKind, chunk_code_chunks};

/// The residual-gap floor default (chars). Mirrors
/// `assembly::residual::RESIDUAL_GAP_FLOOR_CHARS_DEFAULT`. Kept as a local
/// literal on purpose: the anti-sliver invariant is a *contract* with that
/// default, and env (`AXON_RESIDUAL_GAP_FLOOR`) is process-global and racy
/// under parallel tests, so we never mutate it here.
const RESIDUAL_GAP_FLOOR: usize = 80;

/// Does any chunk carry a symbol whose (name, kind) matches?
fn has_symbol(chunks: &[CodeChunk], name: &str, kind: SymbolKind) -> bool {
    chunks
        .iter()
        .any(|c| c.symbol_name() == Some(name) && c.symbol_kind() == Some(kind))
}

/// Is there a chunk of this kind carrying *no* name (anonymous decls like Go
/// `const (...)` blocks or interface types whose decl node nulls the name)?
fn has_unnamed_kind(chunks: &[CodeChunk], kind: SymbolKind) -> bool {
    chunks
        .iter()
        .any(|c| c.symbol_kind() == Some(kind) && c.symbol_name().is_none())
}

fn names_kinds(chunks: &[CodeChunk]) -> Vec<(Option<&str>, Option<SymbolKind>)> {
    chunks
        .iter()
        .map(|c| (c.symbol_name(), c.symbol_kind()))
        .collect()
}

// ── 1. Per-language parity matrix ─────────────────────────────────────────
//
// One table-driven test per the 8 supported extensions. Each fixture exercises
// that language's declaration set; we assert the key symbols are captured by
// name+kind in the chunks `chunk_code_chunks` actually produces (the public
// entry — distinct from raw `extract_symbols`, which the extract sidecars cover).

/// A single named expectation against the chunk output.
enum Expect {
    /// A chunk carries this exact (name, kind).
    Named(&'static str, SymbolKind),
    /// A chunk carries this kind with no name (anonymous decl).
    Unnamed(SymbolKind),
}

struct LangCase {
    ext: &'static str,
    src: &'static str,
    expects: &'static [Expect],
}

#[test]
fn per_language_parity_matrix() {
    use Expect::{Named, Unnamed};
    use SymbolKind::{Const, Enum, Function, Method, Mod, Struct, Type};

    let cases: &[LangCase] = &[
        // ── rust: fn + struct + impl method (Type::method) + macro_rules! ──
        LangCase {
            ext: "rs",
            src: "macro_rules! mk {\n    () => {};\n}\n\n\
pub struct Engine {\n    name: String,\n}\n\n\
impl Engine {\n    pub fn run(&self) -> bool {\n        !self.name.is_empty()\n    }\n}\n\n\
fn free_fn() -> i32 {\n    42\n}\n",
            expects: &[
                Named("mk", Mod), // macro_rules! captures as Mod
                Named("Engine", Struct),
                Named("Engine::run", Method),
                Named("free_fn", Function),
            ],
        },
        // ── py: def + async def + @decorated def + class method + lambda assign ──
        LangCase {
            ext: "py",
            src: "import functools\n\n\
def plain():\n    return 1\n\n\
async def af():\n    return 2\n\n\
@functools.cache\ndef decorated():\n    return 3\n\n\
class C:\n    def meth(self):\n        return 4\n\n\
adder = lambda x: x + 1\n",
            expects: &[
                Named("plain", Function),
                Named("af", Function), // async def → Function (no distinct async node)
                Named("decorated", Function), // decorated def spans the @line
                Named("C", Struct),
                Named("C::meth", Method),
                Named("adder", Function), // lambda assignment → Function
            ],
        },
        // ── js: function + class + arrow-fn const ──
        LangCase {
            ext: "js",
            src: "function plain() {\n  return 1;\n}\n\n\
class K {\n  handle(req) {\n    return req;\n  }\n}\n\n\
const Arrow = () => {\n  return 3;\n};\n",
            expects: &[
                Named("plain", Function),
                Named("K", Struct), // JS class → Struct
                Named("K::handle", Method),
                Named("Arrow", Function), // `const F = () => {}` → Function
            ],
        },
        // ── jsx: arrow-fn component (JSX body) ──
        LangCase {
            ext: "jsx",
            src: "const App = () => {\n  return <div>Hello</div>;\n};\n",
            expects: &[Named("App", Function)],
        },
        // ── ts: function + interface + type alias + enum + arrow const + exported const ──
        LangCase {
            ext: "ts",
            src: "export function createClient(): number {\n  return 1;\n}\n\n\
export interface Transport {\n  start(): void;\n}\n\n\
export type Alias = number;\n\n\
export enum Color {\n  Red,\n  Blue,\n}\n\n\
export const Arrow = (): number => 2;\n\n\
export const NUM = 42;\n",
            expects: &[
                Named("createClient", Function),
                Named("Transport", Type), // interface → Type
                Named("Alias", Type),     // type alias → Type
                Named("Color", Enum),
                Named("Arrow", Function),
                Named("NUM", Const), // exported non-fn const → Const
            ],
        },
        // ── tsx: arrow-fn component + plain function. The brace-less direct-JSX
        //    arrow body now captures too (see
        //    brace_less_jsx_arrow_component_captures_in_tsx). ──
        LangCase {
            ext: "tsx",
            src: "export const Widget = (x: number): number => x + 1;\n\n\
export function Other(): number {\n  return 2;\n}\n",
            expects: &[Named("Widget", Function), Named("Other", Function)],
        },
        // ── go: func + receiver method + interface type + const block ──
        LangCase {
            ext: "go",
            src: "package demo\n\n\
type Reader interface {\n\tRead() int\n}\n\n\
type Resp struct{}\n\n\
func (r *Resp) Parse() int {\n\treturn 1\n}\n\n\
func Free() int {\n\treturn 2\n}\n\n\
const (\n\tA = 1\n\tB = 2\n)\n",
            expects: &[
                Unnamed(Type),               // interface decl nulls its name (parity)
                Named("Resp.Parse", Method), // receiver method
                Named("Free", Function),
                Unnamed(Const), // `const (...)` block → one unnamed Const decl
            ],
        },
        // ── sh: both `f() {}` and `function f {}` forms ──
        LangCase {
            ext: "sh",
            src: "paren_form() {\n  echo a\n}\n\n\
function kw_form {\n  echo b\n}\n",
            expects: &[Named("paren_form", Function), Named("kw_form", Function)],
        },
    ];

    for case in cases {
        let chunks = chunk_code_chunks(case.src, case.ext)
            .unwrap_or_else(|| panic!("ext {} should be supported", case.ext));
        for expect in case.expects {
            match expect {
                Named(name, kind) => assert!(
                    has_symbol(&chunks, name, *kind),
                    "[{}] missing symbol {name:?} ({kind:?}); got {:?}",
                    case.ext,
                    names_kinds(&chunks),
                ),
                Unnamed(kind) => assert!(
                    has_unnamed_kind(&chunks, *kind),
                    "[{}] missing unnamed {kind:?}; got {:?}",
                    case.ext,
                    names_kinds(&chunks),
                ),
            }
        }
    }
}

/// Brace-less direct-JSX arrow components (`const C = () => <jsx/>`) — the single
/// most common React component shape — now capture as a Function in `.tsx`. This
/// was previously a known gap (the old `.tsx` route ran the plain-TypeScript
/// grammar, which can't parse JSX, so the arrow degraded to a symbol-less Prose
/// chunk). Routing `.tsx` to the JSX grammar (`Extractor::Tsx` → `LANGUAGE_TSX`,
/// bead axon_rust-gnpr / CodeRabbit #3) closed it: the JSX body parses, the
/// arrow-bound const matches the arrow-fn rule, and `Widget` is captured.
#[test]
fn brace_less_jsx_arrow_component_captures_in_tsx() {
    let src = "export const Widget = ({ x }: Props) => <span>{x}</span>;\n";
    let chunks = chunk_code_chunks(src, "tsx").unwrap();
    assert!(
        has_symbol(&chunks, "Widget", SymbolKind::Function),
        "brace-less JSX-body tsx arrow component must capture as a Function; got {:?}",
        names_kinds(&chunks),
    );
    assert!(!chunks.is_empty());
}

/// The block-body form of a JSX arrow component DOES capture in `.tsx` — the
/// contrast that localizes the gap above to brace-less direct-JSX bodies.
#[test]
fn tsx_block_body_arrow_component_captures() {
    let src = "export const Widget = ({ x }: Props) => {\n  return <span>{x}</span>;\n};\n";
    let chunks = chunk_code_chunks(src, "tsx").unwrap();
    assert!(
        has_symbol(&chunks, "Widget", SymbolKind::Function),
        "tsx block-body arrow component should capture; got {:?}",
        names_kinds(&chunks),
    );
}

/// In `.jsx` the brace-less direct-JSX arrow component captures fine — proving
/// the gap is tsx-route-specific, not a general arrow-with-JSX problem.
#[test]
fn jsx_direct_jsx_arrow_component_captures() {
    let src = "const App = () => <div>Hello</div>;\n";
    let chunks = chunk_code_chunks(src, "jsx").unwrap();
    assert!(
        has_symbol(&chunks, "App", SymbolKind::Function),
        "jsx direct-JSX arrow component should capture; got {:?}",
        names_kinds(&chunks),
    );
}

// ── 2. Anti-sliver invariant (the headline proof) ─────────────────────────
//
// For every chunk produced by a declaration-dense fixture: it MUST either
// carry a symbol OR be at least RESIDUAL_GAP_FLOOR chars after trimming. A
// symbol-less sub-floor chunk is exactly the "sliver" the rework exists to
// kill. This is the measurable contract.

/// Returns the offending slivers (symbol-less AND below the floor), if any.
fn slivers(chunks: &[CodeChunk]) -> Vec<&CodeChunk> {
    chunks
        .iter()
        .filter(|c| c.symbol.is_none() && c.text.trim().chars().count() < RESIDUAL_GAP_FLOOR)
        .collect()
}

#[test]
fn anti_sliver_invariant_holds_across_languages() {
    // Multi-construct fixtures per language, chosen to provoke residual gaps:
    // interleaved blank lines, stray punctuation, closures, JSX expressions,
    // and several small declarations crammed together.
    let fixtures: &[(&str, &str)] = &[
        (
            "rs",
            // impl + closures + free fns + stray punctuation gaps
            "use std::fmt;\n\nconst MAX: usize = 100;\n\n\
struct Engine {\n    name: String,\n}\n\n\
impl Engine {\n    fn run(&self) -> usize {\n        let f = || self.name.len();\n        let g = |x: usize| x + 1;\n        g(f())\n    }\n\n    fn helper(&self) {}\n}\n\n;\n\n\
fn a() {\n    let _ = 1;\n}\n\n}\n\nfn b() {}\n",
        ),
        (
            "tsx",
            // several arrow-fn components + JSX expressions + plain fns
            "import React from 'react';\n\n\
const Header = (): number => 1;\n\n\
const Footer = (n: number): number => n + 1;\n\n\
export function Page(): number {\n  const items = [1, 2, 3];\n  return items.length;\n}\n\n\
const Aside = (x: number): number => {\n  const doubled = x * 2;\n  return doubled;\n};\n",
        ),
        (
            "ts",
            "export const A = (): number => 1;\nexport const B = (): number => 2;\n\n\
export interface I {\n  f(): void;\n}\n\nexport type T = number;\n\n\
export enum E {\n  X,\n  Y,\n}\n\nexport const NUM = 7;\n",
        ),
        (
            "py",
            "import os\n\n\
@property\ndef decorated():\n    return 1\n\n\
async def af():\n    return 2\n\n\
class C:\n    def m(self):\n        return 3\n\nf = lambda x: x\n\ng = lambda y: y + 1\n",
        ),
        (
            "go",
            "package demo\n\n\
type R interface {\n\tRead() int\n}\n\n\
func (r *Resp) M() int {\n\tf := func() int { return 1 }\n\treturn f()\n}\n\n\
func Free() {}\n\nconst (\n\tA = 1\n\tB = 2\n)\n",
        ),
        (
            "js",
            "const A = () => 1;\nconst B = () => 2;\n\n\
class K {\n  m() {}\n}\n\nfunction plain() {}\n\n;\n\nconst C = () => {\n  return 3;\n};\n",
        ),
        (
            "sh",
            "set -euo pipefail\n\nfirst() {\n  echo a\n}\n\nfunction second {\n  echo b\n}\n\nfirst\nsecond\n",
        ),
    ];

    for (ext, src) in fixtures {
        let chunks = chunk_code_chunks(src, ext).unwrap();
        assert!(!chunks.is_empty(), "[{ext}] produced zero chunks");
        let offenders = slivers(&chunks);
        assert!(
            offenders.is_empty(),
            "[{ext}] ANTI-SLIVER VIOLATION: {} symbol-less sub-{RESIDUAL_GAP_FLOOR}-char chunk(s): {:?}",
            offenders.len(),
            offenders
                .iter()
                .map(|c| (c.start_line, c.end_line, c.text.trim()))
                .collect::<Vec<_>>(),
        );
    }
}

// ── 3. Multibyte safety ───────────────────────────────────────────────────
//
// Byte-range slicing on a non-char-boundary panics. These fixtures put
// non-ASCII content (CJK, emoji, em-dash) in docstrings, string literals,
// identifiers, and comments to exercise the slicing/locate paths.

#[test]
fn multibyte_python_cjk_emoji_docstring_no_panic() {
    let src = "def 处理(数据):\n    \"\"\"处理数据 — handles データ 🚀 with flair.\"\"\"\n    return 数据\n\n\
class 类:\n    def 方法(self):\n        return \"絵文字 😀 ✨\"\n";
    let chunks = chunk_code_chunks(src, "py").unwrap();
    assert!(!chunks.is_empty());
    assert!(chunks.iter().all(|c| !c.text.is_empty()));
    for c in &chunks {
        assert!(c.start_line >= 1 && c.start_line <= c.end_line);
    }
}

#[test]
fn multibyte_rust_emdash_and_cjk_string_literal_no_panic() {
    let src = "/// Doc — with an em-dash and 日本語.\nfn 関数() {\n    let s = \"café — 日本語 🦀 emoji\";\n    println!(\"{s}\");\n}\n\n\
const 定数: &str = \"値 — value\";\n";
    let chunks = chunk_code_chunks(src, "rs").unwrap();
    assert!(!chunks.is_empty());
    assert!(chunks.iter().all(|c| !c.text.is_empty()));
    for c in &chunks {
        assert!(c.start_line >= 1 && c.start_line <= c.end_line);
    }
}

#[test]
fn multibyte_js_unicode_identifier_and_emoji_comment_no_panic() {
    let src = "// 这是一个注释 — emoji 🎉 in a comment\nfunction café() {\n  return 'naïve — résumé';\n}\n\n\
const Ωmega = () => {\n  // 日本語コメント 😀\n  return 1;\n};\n";
    let chunks = chunk_code_chunks(src, "js").unwrap();
    assert!(!chunks.is_empty());
    assert!(chunks.iter().all(|c| !c.text.is_empty()));
    for c in &chunks {
        assert!(c.start_line >= 1 && c.start_line <= c.end_line);
    }
}

// ── 4. Declaration-sparse / fallback files ────────────────────────────────

#[test]
fn rust_barrel_reexports_fall_back_to_nonempty_prose() {
    let src = "pub use crate::a::Foo;\npub use crate::b::Bar;\npub use crate::c::Baz;\npub use crate::d::Qux;\n";
    let chunks = chunk_code_chunks(src, "rs").unwrap();
    assert!(!chunks.is_empty(), "barrel file must never be zero chunks");
    assert!(chunks.iter().all(|c| c.source == ChunkSource::Prose));
}

#[test]
fn ts_barrel_reexports_fall_back_to_nonempty_prose() {
    let src = "export { a } from './a';\nexport { b } from './b';\nexport { c } from './c';\nexport { d } from './d';\n";
    let chunks = chunk_code_chunks(src, "ts").unwrap();
    assert!(
        !chunks.is_empty(),
        "ts barrel file must never be zero chunks"
    );
    // No declaration symbols are extractable from pure re-exports → prose.
    assert!(chunks.iter().all(|c| c.source == ChunkSource::Prose));
}

#[test]
fn all_comments_file_falls_back_to_nonempty_prose() {
    let src = "// This file is all comments.\n// It carries no declarations at all.\n\
// Yet it must still produce non-empty output rather than zero chunks.\n// Line four of commentary.\n";
    let chunks = chunk_code_chunks(src, "rs").unwrap();
    assert!(
        !chunks.is_empty(),
        "all-comments file must never be zero chunks"
    );
    assert!(chunks.iter().all(|c| c.source == ChunkSource::Prose));
}

#[test]
fn empty_file_is_empty_vec_every_language() {
    for ext in ["rs", "py", "js", "jsx", "ts", "tsx", "go", "sh"] {
        assert!(
            chunk_code_chunks("", ext).unwrap().is_empty(),
            "[{ext}] empty file must be vec![]"
        );
    }
}

#[test]
fn whitespace_only_file_is_empty_vec_every_language() {
    for ext in ["rs", "py", "js", "jsx", "ts", "tsx", "go", "sh"] {
        assert!(
            chunk_code_chunks("  \n\n\t  \n", ext).unwrap().is_empty(),
            "[{ext}] whitespace-only file must be vec![]"
        );
    }
}

// ── 5. Range correctness (1-indexed) ──────────────────────────────────────

#[test]
fn line_ranges_are_one_indexed_and_accurate() {
    // Line 1: comment. Lines 2-4: struct. Lines 6-8: fn.
    let src = "// leading comment\nstruct Point {\n    x: i32,\n}\n\n\
fn make() -> Point {\n    Point { x: 0 }\n}\n";
    let chunks = chunk_code_chunks(src, "rs").unwrap();

    let point = chunks
        .iter()
        .find(|c| c.symbol_name() == Some("Point"))
        .expect("Point struct should be captured");
    // The leading comment is attached, so the chunk starts at line 1, but the
    // *declaration* range points at the struct itself (lines 2-4).
    assert_eq!(point.declaration_start_line, 2, "Point decl starts line 2");
    assert_eq!(point.declaration_end_line, 4, "Point decl ends line 4");

    let make = chunks
        .iter()
        .find(|c| c.symbol_name() == Some("make"))
        .expect("make fn should be captured");
    assert_eq!(make.declaration_start_line, 6, "make decl starts line 6");
    assert_eq!(make.declaration_end_line, 8, "make decl ends line 8");
    assert!(make.start_line >= 1 && make.start_line <= make.end_line);
}

#[test]
fn crlf_line_endings_with_leading_comment_keep_accurate_ranges() {
    // CRLF endings must not shift line accounting; leading comment on line 1,
    // function declaration on lines 2-4.
    let src = "// header comment\r\nfn worker() {\r\n    let _ = 1;\r\n}\r\n";
    let chunks = chunk_code_chunks(src, "rs").unwrap();
    let worker = chunks
        .iter()
        .find(|c| c.symbol_name() == Some("worker"))
        .expect("worker fn should be captured under CRLF");
    assert_eq!(
        worker.declaration_start_line, 2,
        "CRLF: worker decl starts line 2"
    );
    assert_eq!(
        worker.declaration_end_line, 4,
        "CRLF: worker decl ends line 4"
    );
    assert!(worker.start_line >= 1 && worker.start_line <= worker.end_line);
}

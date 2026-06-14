use std::cell::RefCell;
use std::sync::LazyLock;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor};
use tree_sitter_language::LanguageFn;

use super::chunk::SymbolKind;

mod bash;
mod go;
mod js_ts;
mod python;
mod rust;

#[derive(Debug, Clone)]
pub(super) struct SymbolInfo {
    pub(super) byte_start: usize,
    pub(super) byte_end: usize,
    pub(super) start_line: u32,
    pub(super) end_line: u32,
    pub(super) name: Option<String>,
    pub(super) kind: SymbolKind,
    /// Whether this declaration can contain methods (Container) or is a leaf.
    /// Additive: callers (code.rs) ignore this today; a later bead consumes it.
    #[allow(dead_code)]
    pub(super) role: DeclRole,
}

/// Whether a declaration can contain nested method declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DeclRole {
    /// A function/method/const/type/etc. — does not contain methods.
    Leaf,
    /// A class/impl/trait/mod/etc. — can contain methods.
    Container,
}

/// A data-driven declaration extraction rule: a tree-sitter query S-expression
/// with `@decl` (full declaration node) and `@name` (identifier) captures, plus
/// the symbol kind and container role it produces.
pub(super) struct DeclRule {
    pub(super) pattern: &'static str,
    pub(super) kind: SymbolKind,
    pub(super) role: DeclRole,
}

/// A compiled [`DeclRule`]: query compiled once, paired with its kind/role.
pub(super) struct CompiledRule {
    query: Query,
    kind: SymbolKind,
    role: DeclRole,
}

impl CompiledRule {
    fn compile(language: &Language, rule: &DeclRule) -> Self {
        let query = Query::new(language, rule.pattern)
            .unwrap_or_else(|err| panic!("invalid tree-sitter query {:?}: {err}", rule.pattern));
        CompiledRule {
            query,
            kind: rule.kind,
            role: rule.role,
        }
    }
}

/// Per-language refinement hook: given the matched declaration node and its raw
/// name, returns the final (kind, name) — handling Method-vs-Function
/// reclassification and parent qualification by walking the AST.
type Refine = fn(Node<'_>, &str, &str, SymbolKind) -> (SymbolKind, Option<String>);

#[derive(Clone, Copy)]
pub(super) struct LanguageSpec {
    pub(super) grammar: LanguageFn,
    pub(super) extractor: Extractor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Extractor {
    None,
    Rust,
    Go,
    Python,
    JavaScript,
    TypeScript,
    Bash,
}

pub(super) fn language_for_extension(ext: &str) -> Option<LanguageSpec> {
    match ext {
        "rs" => Some(LanguageSpec {
            grammar: tree_sitter_rust::LANGUAGE,
            extractor: Extractor::Rust,
        }),
        "py" => Some(LanguageSpec {
            grammar: tree_sitter_python::LANGUAGE,
            extractor: Extractor::Python,
        }),
        "js" | "jsx" => Some(LanguageSpec {
            grammar: tree_sitter_javascript::LANGUAGE,
            extractor: Extractor::JavaScript,
        }),
        "ts" => Some(LanguageSpec {
            grammar: tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            extractor: Extractor::TypeScript,
        }),
        "tsx" => Some(LanguageSpec {
            grammar: tree_sitter_typescript::LANGUAGE_TSX,
            extractor: Extractor::TypeScript,
        }),
        "go" => Some(LanguageSpec {
            grammar: tree_sitter_go::LANGUAGE,
            extractor: Extractor::Go,
        }),
        "sh" | "bash" => Some(LanguageSpec {
            grammar: tree_sitter_bash::LANGUAGE,
            extractor: Extractor::Bash,
        }),
        _ => None,
    }
}

/// A registry entry: the compiled grammar, its compiled rules, and the refine hook.
struct Registry {
    language: Language,
    rules: Vec<CompiledRule>,
    refine: Refine,
}

impl Registry {
    fn new(grammar: LanguageFn, rules: &'static [DeclRule], refine: Refine) -> Self {
        let language = Language::from(grammar);
        let compiled = rules
            .iter()
            .map(|rule| CompiledRule::compile(&language, rule))
            .collect();
        Registry {
            language,
            rules: compiled,
            refine,
        }
    }
}

static RUST_REGISTRY: LazyLock<Registry> =
    LazyLock::new(|| Registry::new(tree_sitter_rust::LANGUAGE, rust::RULES, rust::refine));
static GO_REGISTRY: LazyLock<Registry> =
    LazyLock::new(|| Registry::new(tree_sitter_go::LANGUAGE, go::RULES, go::refine));
static PYTHON_REGISTRY: LazyLock<Registry> =
    LazyLock::new(|| Registry::new(tree_sitter_python::LANGUAGE, python::RULES, python::refine));
static JAVASCRIPT_REGISTRY: LazyLock<Registry> = LazyLock::new(|| {
    Registry::new(
        tree_sitter_javascript::LANGUAGE,
        js_ts::JS_RULES,
        js_ts::refine,
    )
});
static TYPESCRIPT_REGISTRY: LazyLock<Registry> = LazyLock::new(|| {
    Registry::new(
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
        js_ts::TS_RULES,
        js_ts::refine,
    )
});
static TSX_REGISTRY: LazyLock<Registry> = LazyLock::new(|| {
    Registry::new(
        tree_sitter_typescript::LANGUAGE_TSX,
        js_ts::TS_RULES,
        js_ts::refine,
    )
});
static BASH_REGISTRY: LazyLock<Registry> =
    LazyLock::new(|| Registry::new(tree_sitter_bash::LANGUAGE, bash::RULES, bash::refine));

thread_local! {
    static PARSER: RefCell<Parser> = RefCell::new(Parser::new());
}

fn registry_for(extractor: Extractor, ext: Option<&str>) -> Option<&'static Registry> {
    match extractor {
        Extractor::Rust => Some(&RUST_REGISTRY),
        Extractor::Go => Some(&GO_REGISTRY),
        Extractor::Python => Some(&PYTHON_REGISTRY),
        Extractor::JavaScript => Some(&JAVASCRIPT_REGISTRY),
        Extractor::TypeScript => match ext {
            Some("tsx") => Some(&TSX_REGISTRY),
            _ => Some(&TYPESCRIPT_REGISTRY),
        },
        Extractor::Bash => Some(&BASH_REGISTRY),
        Extractor::None => None,
    }
}

pub(super) fn extract_symbols(content: &str, extractor: Extractor) -> Vec<SymbolInfo> {
    let Some(registry) = registry_for(extractor, None) else {
        return Vec::new();
    };
    PARSER.with(|slot| {
        let mut parser = slot.borrow_mut();
        parser.reset();
        if parser.set_language(&registry.language).is_err() {
            return Vec::new();
        }
        let Some(tree) = parser.parse(content, None) else {
            return Vec::new();
        };
        let root = tree.root_node();
        let mut symbols = run_rules(registry, root, content);
        dedup_by_exact_range(&mut symbols);
        symbols.sort_by_key(|sym| (sym.byte_start, sym.byte_end));
        symbols
    })
}

fn run_rules(registry: &Registry, root: Node<'_>, content: &str) -> Vec<SymbolInfo> {
    let bytes = content.as_bytes();
    let mut symbols = Vec::new();
    let mut cursor = QueryCursor::new();
    for rule in &registry.rules {
        let decl_idx = capture_index(&rule.query, "decl");
        let name_idx = capture_index(&rule.query, "name");
        let mut matches = cursor.matches(&rule.query, root, bytes);
        while let Some(m) = matches.next() {
            let mut decl_node = None;
            let mut name_node = None;
            for cap in m.captures {
                if Some(cap.index) == decl_idx {
                    decl_node = Some(cap.node);
                } else if Some(cap.index) == name_idx {
                    name_node = Some(cap.node);
                }
            }
            let Some(decl) = decl_node else { continue };
            let Some(name) = name_node else { continue };
            let raw_name = name.utf8_text(bytes).unwrap_or("").trim();
            if raw_name.is_empty() {
                continue;
            }
            let (kind, final_name) = (registry.refine)(decl, raw_name, content, rule.kind);
            symbols.push(SymbolInfo {
                byte_start: decl.start_byte(),
                byte_end: decl.end_byte(),
                start_line: decl.start_position().row as u32 + 1,
                end_line: decl.end_position().row as u32 + 1,
                name: final_name,
                kind,
                role: rule.role,
            });
        }
    }
    symbols
}

fn capture_index(query: &Query, name: &str) -> Option<u32> {
    query
        .capture_names()
        .iter()
        .position(|cap| *cap == name)
        .map(|idx| idx as u32)
}

/// Drop duplicate symbols that capture the exact same `@decl` byte range,
/// keeping the last-encountered one (rules run general → specific).
fn dedup_by_exact_range(symbols: &mut Vec<SymbolInfo>) {
    if symbols.len() <= 1 {
        return;
    }
    let mut keep: Vec<bool> = vec![true; symbols.len()];
    for i in 0..symbols.len() {
        if !keep[i] {
            continue;
        }
        for j in (i + 1)..symbols.len() {
            if symbols[j].byte_start == symbols[i].byte_start
                && symbols[j].byte_end == symbols[i].byte_end
            {
                // later (more specific) wins; drop the earlier one
                keep[i] = false;
                break;
            }
        }
    }
    let mut idx = 0;
    symbols.retain(|_| {
        let k = keep[idx];
        idx += 1;
        k
    });
}

pub(super) fn find_symbol_for_chunk(
    symbols: &[SymbolInfo],
    chunk_start: usize,
    chunk_end: usize,
) -> Option<&SymbolInfo> {
    symbols
        .iter()
        .filter(|sym| sym.byte_start <= chunk_start && sym.byte_end >= chunk_end)
        .min_by_key(|sym| sym.byte_end.saturating_sub(sym.byte_start))
        .or_else(|| {
            symbols
                .iter()
                .find(|sym| sym.byte_start < chunk_end && sym.byte_end > chunk_start)
        })
}

// ---- shared refine helpers, reused by the per-language modules ----

/// Extract the `name` field text of a node, trimmed and non-empty.
pub(super) fn node_name(node: Node<'_>, content: &str) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(content.as_bytes()).ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

/// Strip leading reference/pointer sigils from a type fragment.
pub(super) fn clean_symbol_fragment(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('&')
        .trim_start_matches('*')
        .trim()
        .to_string()
}

#[cfg(test)]
#[path = "extract_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "extract_registry_tests.rs"]
mod registry_tests;

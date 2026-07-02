//! syn-based extraction of a crate's true public API surface.
//!
//! Starting from `src/lib.rs`, we walk only modules reachable via an all-`pub`
//! module path (a `pub` item inside a `pub(crate)` or private module is NOT
//! crate-public, so those subtrees are skipped). `pub(crate)`/`pub(super)`/
//! `pub(in …)` visibilities are excluded; `#[cfg(test)]` modules are skipped.
//! File modules follow the repo's `mod.rs`-free convention: `mod foo;` in a file
//! whose module directory is `D` resolves to `D/foo.rs`, and `foo`'s own
//! submodules live in `D/foo/`.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use syn::{Item, UseTree, Visibility};

/// One public API entry: a crate-root-relative path plus the item kind.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ApiEntry {
    pub path: String,
    pub kind: &'static str,
}

/// Extract the public API surface of the crate rooted at `crate_dir`
/// (the directory containing `src/lib.rs`). Crates with no `lib.rs`
/// (binary-only) yield an empty surface.
pub fn extract_crate(crate_dir: &Path) -> Result<Vec<ApiEntry>> {
    let lib = crate_dir.join("src").join("lib.rs");
    if !lib.is_file() {
        return Ok(Vec::new());
    }
    let mut out = BTreeSet::new();
    walk_file(&lib, &crate_dir.join("src"), &[], &mut out)?;
    Ok(out.into_iter().collect())
}

fn walk_file(
    file: &Path,
    mod_dir: &Path,
    prefix: &[String],
    out: &mut BTreeSet<ApiEntry>,
) -> Result<()> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    let ast =
        syn::parse_file(&text).with_context(|| format!("parsing {} as Rust", file.display()))?;
    walk_items(&ast.items, mod_dir, prefix, out)
}

fn walk_items(
    items: &[Item],
    mod_dir: &Path,
    prefix: &[String],
    out: &mut BTreeSet<ApiEntry>,
) -> Result<()> {
    for item in items {
        if has_cfg_test(item) {
            continue;
        }
        match item {
            Item::Mod(m) if is_public(&m.vis) => {
                let name = m.ident.to_string();
                let mut child_prefix = prefix.to_vec();
                child_prefix.push(name.clone());
                record(out, &child_prefix, "mod");
                match &m.content {
                    // Inline `pub mod foo { … }`: submodule dir is `mod_dir/foo`.
                    Some((_, inner)) => {
                        walk_items(inner, &mod_dir.join(&name), &child_prefix, out)?;
                    }
                    // File `pub mod foo;`: `mod_dir/foo.rs`, submodules in `mod_dir/foo/`.
                    None => {
                        let child_file = mod_dir.join(format!("{name}.rs"));
                        if child_file.is_file() {
                            walk_file(&child_file, &mod_dir.join(&name), &child_prefix, out)?;
                        }
                    }
                }
            }
            Item::Fn(i) if is_public(&i.vis) => record_named(out, prefix, &i.sig.ident, "fn"),
            Item::Struct(i) if is_public(&i.vis) => record_named(out, prefix, &i.ident, "struct"),
            Item::Enum(i) if is_public(&i.vis) => record_named(out, prefix, &i.ident, "enum"),
            Item::Trait(i) if is_public(&i.vis) => record_named(out, prefix, &i.ident, "trait"),
            Item::Type(i) if is_public(&i.vis) => record_named(out, prefix, &i.ident, "type"),
            Item::Const(i) if is_public(&i.vis) => record_named(out, prefix, &i.ident, "const"),
            Item::Static(i) if is_public(&i.vis) => record_named(out, prefix, &i.ident, "static"),
            Item::Union(i) if is_public(&i.vis) => record_named(out, prefix, &i.ident, "union"),
            Item::TraitAlias(i) if is_public(&i.vis) => {
                record_named(out, prefix, &i.ident, "trait-alias")
            }
            Item::Use(u) if is_public(&u.vis) => {
                let mut leaves = Vec::new();
                collect_use_leaves(&u.tree, &mut leaves);
                for leaf in leaves {
                    let mut p = prefix.to_vec();
                    p.push(leaf);
                    record(out, &p, "use");
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Only bare `pub` counts as crate-public API. `pub(crate)`, `pub(super)`,
/// `pub(in …)` are `Visibility::Restricted`; private is `Inherited`.
fn is_public(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

fn has_cfg_test(item: &Item) -> bool {
    let attrs = match item {
        Item::Mod(m) => &m.attrs,
        Item::Fn(f) => &f.attrs,
        _ => return false,
    };
    attrs.iter().any(|a| {
        if !a.path().is_ident("cfg") {
            return false;
        }
        // Match `#[cfg(test)]` and `#[cfg(all(test, …))]` by substring on the
        // token stream — good enough to exclude test-only modules.
        a.meta
            .require_list()
            .map(|l| l.tokens.to_string().contains("test"))
            .unwrap_or(false)
    })
}

/// Collect the public leaf names introduced by a `pub use` tree (the `as` name
/// for renames, `*` for globs), ignoring the source path segments.
fn collect_use_leaves(tree: &UseTree, out: &mut Vec<String>) {
    match tree {
        UseTree::Path(p) => collect_use_leaves(&p.tree, out),
        UseTree::Name(n) => out.push(n.ident.to_string()),
        UseTree::Rename(r) => out.push(r.rename.to_string()),
        UseTree::Glob(_) => out.push("*".to_string()),
        UseTree::Group(g) => {
            for t in &g.items {
                collect_use_leaves(t, out);
            }
        }
    }
}

fn record_named(
    out: &mut BTreeSet<ApiEntry>,
    prefix: &[String],
    ident: &syn::Ident,
    kind: &'static str,
) {
    let mut p = prefix.to_vec();
    p.push(ident.to_string());
    record(out, &p, kind);
}

fn record(out: &mut BTreeSet<ApiEntry>, path: &[String], kind: &'static str) {
    out.insert(ApiEntry {
        path: path.join("::"),
        kind,
    });
}

#[cfg(test)]
#[path = "extract_tests.rs"]
mod tests;

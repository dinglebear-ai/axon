use std::collections::{BTreeMap, BTreeSet};

use syn::{Item, ItemExternCrate, ItemUse, Stmt, UseTree};

use super::cfg::item_is_test_only;

#[derive(Clone, Debug)]
pub(super) struct Import {
    pub local: Option<String>,
    pub canonical: Vec<String>,
    pub glob: bool,
}

#[derive(Default)]
pub(super) struct AliasStack {
    scopes: Vec<BTreeMap<String, Vec<String>>>,
}

impl AliasStack {
    pub fn push_items(&mut self, items: &[Item]) {
        let mut scope = BTreeMap::new();
        for item in items.iter().filter(|item| !item_is_test_only(item)) {
            collect_item_aliases(item, &mut scope);
        }
        self.scopes.push(scope);
    }

    pub fn push_stmts(&mut self, stmts: &[Stmt]) {
        let mut scope = BTreeMap::new();
        for stmt in stmts {
            if let Stmt::Item(item) = stmt
                && !item_is_test_only(item)
            {
                collect_item_aliases(item, &mut scope);
            }
        }
        self.scopes.push(scope);
    }

    pub fn pop(&mut self) {
        self.scopes.pop();
    }

    pub fn resolve(&self, path: &[String]) -> Vec<String> {
        let mut resolved = path.to_vec();
        let mut seen = BTreeSet::new();
        while let Some((first, tail)) = resolved.split_first() {
            let Some((scope_index, replacement)) = self.lookup(first) else {
                break;
            };
            if !seen.insert((scope_index, first.clone())) {
                break;
            }
            resolved = replacement
                .iter()
                .cloned()
                .chain(tail.iter().cloned())
                .collect();
        }
        resolved
    }

    fn lookup(&self, local: &str) -> Option<(usize, &Vec<String>)> {
        self.scopes
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, scope)| scope.get(local).map(|path| (index, path)))
    }
}

pub(super) fn imports_from_use(item: &ItemUse) -> Vec<Import> {
    let mut imports = Vec::new();
    flatten_use_tree(&item.tree, Vec::new(), &mut imports);
    imports
}

pub(super) fn import_from_extern_crate(item: &ItemExternCrate) -> Import {
    Import {
        local: Some(
            item.rename
                .as_ref()
                .map_or_else(|| item.ident.to_string(), |(_, rename)| rename.to_string()),
        ),
        canonical: vec![item.ident.to_string()],
        glob: false,
    }
}

fn collect_item_aliases(item: &Item, scope: &mut BTreeMap<String, Vec<String>>) {
    match item {
        Item::Use(item) => {
            for import in imports_from_use(item) {
                if let Some(local) = import.local {
                    scope.insert(local, import.canonical);
                }
            }
        }
        Item::ExternCrate(item) => {
            let import = import_from_extern_crate(item);
            scope.insert(
                import.local.expect("extern crate has local name"),
                import.canonical,
            );
        }
        _ => {}
    }
}

fn flatten_use_tree(tree: &UseTree, prefix: Vec<String>, output: &mut Vec<Import>) {
    match tree {
        UseTree::Path(path) => {
            let mut next = prefix;
            next.push(path.ident.to_string());
            flatten_use_tree(&path.tree, next, output);
        }
        UseTree::Name(name) if name.ident == "self" => {
            output.push(Import {
                local: prefix.last().cloned(),
                canonical: prefix,
                glob: false,
            });
        }
        UseTree::Name(name) => {
            let mut canonical = prefix;
            canonical.push(name.ident.to_string());
            output.push(Import {
                local: Some(name.ident.to_string()),
                canonical,
                glob: false,
            });
        }
        UseTree::Rename(rename) if rename.ident == "self" => {
            output.push(Import {
                local: Some(rename.rename.to_string()),
                canonical: prefix,
                glob: false,
            });
        }
        UseTree::Rename(rename) => {
            let mut canonical = prefix;
            canonical.push(rename.ident.to_string());
            output.push(Import {
                local: Some(rename.rename.to_string()),
                canonical,
                glob: false,
            });
        }
        UseTree::Glob(_) => output.push(Import {
            local: None,
            canonical: prefix,
            glob: true,
        }),
        UseTree::Group(group) => {
            for item in &group.items {
                flatten_use_tree(item, prefix.clone(), output);
            }
        }
    }
}

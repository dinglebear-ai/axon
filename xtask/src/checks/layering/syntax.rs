use std::collections::BTreeMap;

use syn::visit::Visit;
use syn::{ExprField, ExprMethodCall, ItemUse, Member, Path, UseTree};

use super::{Finding, ReachRule};

const PROVIDER_TYPES: &[&str] = &[
    "EmbeddingProvider",
    "VectorStore",
    "FetchProvider",
    "RenderProvider",
    "GraphStore",
    "ArtifactStore",
    "LlmProvider",
];

const PROVIDER_HANDLES: &[&str] = &[
    "embedding_provider",
    "vector_store",
    "fetch_provider",
    "render_provider",
    "graph_store",
    "artifact_store",
    "llm_provider",
];

// Only names specific enough to avoid rejecting unrelated domain methods.
// Collision-prone operations (`get`, `delete`, `reset`, `query`, `search`,
// `resolve`, `capabilities`, and `fetch`) are enforced through the provider
// type/import or provider-handle boundary instead.
const DISTINCT_PROVIDER_OPERATIONS: &[&str] = &[
    "embed",
    "ensure_collection",
    "upsert",
    "mark_generation_committed",
    "mark_unchanged_items_committed",
    "render",
    "upsert_candidates",
    "put_bytes",
    "complete",
    "complete_streaming",
];

pub(super) fn scan(syntax: &syn::File, rel: &str, reach_rules: &[ReachRule]) -> Vec<Finding> {
    let mut scanner = Scanner {
        rel,
        reach_rules,
        aliases: BTreeMap::new(),
        findings: Vec::new(),
    };
    scanner.visit_file(syntax);
    scanner.findings
}

struct Scanner<'a> {
    rel: &'a str,
    reach_rules: &'a [ReachRule],
    aliases: BTreeMap<String, Vec<String>>,
    findings: Vec<Finding>,
}

impl Scanner<'_> {
    fn record(&mut self, rule: impl Into<String>, detail: impl Into<String>) {
        self.findings.push(Finding {
            path: self.rel.to_owned(),
            rule: rule.into(),
            detail: detail.into(),
        });
    }

    fn inspect_path(&mut self, original: Vec<String>) {
        let resolved = resolve_alias(&original, &self.aliases);
        let rendered = resolved.join("::");

        for reach in self.reach_rules {
            if path_has_prefix(&resolved, reach.prefix) {
                self.record(
                    format!("reach:{}", reach.prefix.join("::")),
                    format!("reaches {} `{rendered}`", reach.kind),
                );
            }
        }

        if let Some(provider) = resolved
            .last()
            .filter(|name| PROVIDER_TYPES.contains(&name.as_str()))
        {
            self.record(
                format!("provider-type:{provider}"),
                format!("uses raw provider type `{rendered}`"),
            );
        }

        if resolved.len() >= 2 {
            let operation = resolved.last().expect("non-empty path");
            let owner = &resolved[resolved.len() - 2];
            if PROVIDER_TYPES.contains(&owner.as_str())
                && DISTINCT_PROVIDER_OPERATIONS.contains(&operation.as_str())
            {
                self.record(
                    format!("provider-op:{owner}::{operation}"),
                    format!("calls raw provider operation `{rendered}`"),
                );
            }
        }

        if resolved.first().map(String::as_str) == Some("axon_llm")
            && matches!(
                resolved.last().map(String::as_str),
                Some("complete_text" | "complete_streaming")
            )
        {
            let operation = resolved.last().expect("matched LLM operation");
            self.record(
                format!("provider-op:axon_llm::{operation}"),
                format!("calls raw LLM provider operation `{rendered}`"),
            );
        }
    }
}

impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        let mut imports = Vec::new();
        flatten_use_tree(&node.tree, Vec::new(), &mut imports);
        for (local, canonical) in imports {
            self.aliases.insert(local, canonical.clone());
            self.inspect_path(canonical);
        }
        // Imports were flattened above. Do not traverse them again and double
        // count grouped, renamed, or multiline entries.
    }

    fn visit_path(&mut self, node: &'ast Path) {
        self.inspect_path(path_segments(node));
        syn::visit::visit_path(self, node);
    }

    fn visit_expr_field(&mut self, node: &'ast ExprField) {
        if let Member::Named(member) = &node.member {
            let name = member.to_string();
            if PROVIDER_HANDLES.contains(&name.as_str()) {
                self.record(
                    format!("provider-handle:{name}"),
                    format!("accesses raw provider handle `.{name}`"),
                );
            }
        }
        syn::visit::visit_expr_field(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        let name = node.method.to_string();
        if PROVIDER_HANDLES.contains(&name.as_str()) {
            self.record(
                format!("provider-handle:{name}"),
                format!("accesses raw provider handle `.{name}(...)`"),
            );
        }
        syn::visit::visit_expr_method_call(self, node);
    }
}

fn path_segments(path: &Path) -> Vec<String> {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect()
}

fn resolve_alias(path: &[String], aliases: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let Some((first, tail)) = path.split_first() else {
        return Vec::new();
    };
    let Some(prefix) = aliases.get(first) else {
        return path.to_vec();
    };
    prefix.iter().cloned().chain(tail.iter().cloned()).collect()
}

fn path_has_prefix(path: &[String], prefix: &[&str]) -> bool {
    path.len() >= prefix.len()
        && path
            .iter()
            .zip(prefix)
            .all(|(segment, expected)| segment == expected)
}

fn flatten_use_tree(tree: &UseTree, prefix: Vec<String>, output: &mut Vec<(String, Vec<String>)>) {
    match tree {
        UseTree::Path(path) => {
            let mut next = prefix;
            next.push(path.ident.to_string());
            flatten_use_tree(&path.tree, next, output);
        }
        UseTree::Name(name) => {
            let mut canonical = prefix;
            canonical.push(name.ident.to_string());
            output.push((name.ident.to_string(), canonical));
        }
        UseTree::Rename(rename) => {
            let mut canonical = prefix;
            canonical.push(rename.ident.to_string());
            output.push((rename.rename.to_string(), canonical));
        }
        UseTree::Glob(_) => {
            if let Some(local) = prefix.last() {
                output.push((local.clone(), prefix));
            }
        }
        UseTree::Group(group) => {
            for item in &group.items {
                flatten_use_tree(item, prefix.clone(), output);
            }
        }
    }
}

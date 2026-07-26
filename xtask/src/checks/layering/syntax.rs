mod aliases;
mod bindings;
mod cfg;
mod tokens;

use aliases::{AliasStack, import_from_extern_crate, imports_from_use};
use bindings::ProviderBindings;
use syn::visit::Visit;
use syn::{
    Arm, Block, ExprClosure, ExprField, ExprMethodCall, FieldPat, FieldValue, ImplItem, ImplItemFn,
    Item, ItemExternCrate, ItemFn, ItemMod, ItemUse, Local, Macro, Member, Path, ReturnType,
    TraitItem, TraitItemFn,
};
use tokens::TokenFinding;

use super::{Finding, ReachRule};

const PROVIDER_TYPES: &[&str] = &[
    "EmbeddingProvider",
    "VectorStore",
    "SearchProvider",
    "FetchProvider",
    "RenderProvider",
    "NetworkCaptureProvider",
    "GraphStore",
    "ArtifactStore",
    "LlmProvider",
];

const CONCRETE_PROVIDER_TYPES: &[&str] = &[
    "QdrantVectorStore",
    "FakeVectorStore",
    "TeiEmbeddingProvider",
    "OpenAiCompatEmbeddingProvider",
    "FakeEmbeddingProvider",
    "SearxngSearchProvider",
    "TavilySearchProvider",
    "HttpFetchProvider",
    "ChromeRenderProvider",
    "FakeAdapterProviders",
    "ChromeNetworkCapture",
    "FileArtifactStore",
    "FakeCoreBoundaries",
    "SqliteGraphStore",
    "FakeGraphStore",
    "FakeLlmProvider",
];

const PROVIDER_HANDLES: &[&str] = &[
    "embedding_provider",
    "vector_store",
    "search_provider",
    "fetch_provider",
    "render_provider",
    "network_capture_provider",
    "capture_provider",
    "graph_store",
    "artifact_store",
    "llm_provider",
];

// Names specific enough to reject independent of receiver type. Collision-
// prone operations remain enforceable whenever the receiver handle/type or
// provider-qualified UFCS path is present.
const LOW_COLLISION_PROVIDER_METHODS: &[&str] = &[
    "embed",
    "ensure_collection",
    "mark_generation_committed",
    "mark_unchanged_items_committed",
    "upsert_candidates",
    "put_bytes",
    "complete_streaming",
    "node_edges",
    "nodes_for_source",
    "delete_nodes",
    "delete_edges",
];

const PROVIDER_GLOB_ROOTS: &[&str] = &[
    "axon_adapters",
    "axon_embedding",
    "axon_graph",
    "axon_llm",
    "axon_vectors",
];

pub(super) fn scan(syntax: &syn::File, rel: &str, reach_rules: &[ReachRule]) -> Vec<Finding> {
    let mut scanner = Scanner {
        rel,
        reach_rules,
        aliases: AliasStack::default(),
        bindings: ProviderBindings::default(),
        findings: Vec::new(),
    };
    scanner.visit_file(syntax);
    scanner.findings
}

pub(super) struct ExternalModule {
    pub name: String,
    pub path_override: Option<String>,
    pub test_only: bool,
}

pub(super) fn external_modules(syntax: &syn::File) -> Vec<ExternalModule> {
    syntax
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Mod(module) if module.content.is_none() => Some(ExternalModule {
                name: module.ident.to_string(),
                path_override: module.attrs.iter().find_map(|attribute| {
                    if !attribute.path().is_ident("path") {
                        return None;
                    }
                    let syn::Meta::NameValue(name_value) = &attribute.meta else {
                        return None;
                    };
                    let syn::Expr::Lit(expr) = &name_value.value else {
                        return None;
                    };
                    let syn::Lit::Str(path) = &expr.lit else {
                        return None;
                    };
                    Some(path.value())
                }),
                test_only: cfg::item_is_test_only(item),
            }),
            _ => None,
        })
        .collect()
}

struct Scanner<'a> {
    rel: &'a str,
    reach_rules: &'a [ReachRule],
    aliases: AliasStack,
    bindings: ProviderBindings,
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
        let resolved = self.aliases.resolve(&original);
        let rendered = resolved.join("::");

        for reach in self.reach_rules {
            if path_has_prefix(&resolved, reach.prefix) {
                self.record(
                    format!("reach:{}", reach.prefix.join("::")),
                    format!("reaches {} `{rendered}`", reach.kind),
                );
            }
        }

        if let Some(provider) = resolved.last().filter(|name| is_provider_type_name(name)) {
            self.record(
                format!("provider-type:{provider}"),
                format!("uses raw provider type `{rendered}`"),
            );
        }

        if resolved.len() >= 2 {
            let operation = resolved.last().expect("non-empty path");
            let owner = &resolved[resolved.len() - 2];
            if is_provider_type_name(owner) {
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

    fn inspect_member(&mut self, name: &str) {
        if PROVIDER_HANDLES.contains(&name) {
            self.record(
                format!("provider-handle:{name}"),
                format!("accesses raw provider handle `{name}`"),
            );
        }
    }

    fn inspect_method(&mut self, name: &str, provider_receiver: bool) {
        self.inspect_member(name);
        if provider_receiver || LOW_COLLISION_PROVIDER_METHODS.contains(&name) {
            self.record(
                format!("provider-method:{name}"),
                format!("calls reserved provider method `.{name}(...)`"),
            );
        }
    }

    fn inspect_glob(&mut self, canonical: Vec<String>) {
        let resolved = self.aliases.resolve(&canonical);
        if resolved
            .first()
            .is_some_and(|root| PROVIDER_GLOB_ROOTS.contains(&root.as_str()))
        {
            let rendered = resolved.join("::");
            self.record(
                format!("provider-glob:{rendered}"),
                format!("imports provider-bearing glob `{rendered}::*`"),
            );
        }
        self.inspect_path(canonical);
    }

    fn inspect_macro(&mut self, node: &Macro) {
        self.inspect_path(path_segments(&node.path));
        for finding in tokens::scan(&node.tokens) {
            match finding {
                TokenFinding::Path(path) => self.inspect_path(path),
                TokenFinding::Method { receiver, method } => {
                    let provider_receiver = receiver
                        .as_deref()
                        .is_some_and(|name| self.bindings.is_provider(name));
                    self.inspect_method(&method, provider_receiver);
                }
                TokenFinding::Member(member) | TokenFinding::Binding(member) => {
                    self.inspect_member(&member);
                }
            }
        }
    }
}

impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_file(&mut self, node: &'ast syn::File) {
        self.aliases.push_items(&node.items);
        self.bindings.push();
        for item in &node.items {
            self.visit_item(item);
        }
        self.bindings.pop();
        self.aliases.pop();
    }

    fn visit_item(&mut self, node: &'ast Item) {
        if !cfg::item_is_test_only(node) {
            syn::visit::visit_item(self, node);
        }
    }

    fn visit_impl_item(&mut self, node: &'ast ImplItem) {
        if !cfg::impl_item_is_test_only(node) {
            syn::visit::visit_impl_item(self, node);
        }
    }

    fn visit_trait_item(&mut self, node: &'ast TraitItem) {
        if !cfg::trait_item_is_test_only(node) {
            syn::visit::visit_trait_item(self, node);
        }
    }

    fn visit_local(&mut self, node: &'ast Local) {
        if !cfg::attrs_are_test_only(&node.attrs) {
            syn::visit::visit_local(self, node);
            let initialized_as_provider = node
                .init
                .as_ref()
                .is_some_and(|init| self.bindings.expr_is_provider(&self.aliases, &init.expr));
            self.bindings
                .bind_pat(&self.aliases, &node.pat, initialized_as_provider);
        }
    }

    fn visit_arm(&mut self, node: &'ast Arm) {
        if !cfg::attrs_are_test_only(&node.attrs) {
            syn::visit::visit_arm(self, node);
        }
    }

    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        if let Some((_, items)) = &node.content {
            self.aliases.push_items(items);
            self.bindings.push();
            for item in items {
                self.visit_item(item);
            }
            self.bindings.pop();
            self.aliases.pop();
        }
    }

    fn visit_block(&mut self, node: &'ast Block) {
        self.aliases.push_stmts(&node.stmts);
        self.bindings.push();
        for stmt in &node.stmts {
            self.visit_stmt(stmt);
        }
        self.bindings.pop();
        self.aliases.pop();
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.bindings.push();
        self.bindings.bind_inputs(&self.aliases, &node.sig.inputs);
        syn::visit::visit_signature(self, &node.sig);
        self.visit_block(&node.block);
        self.bindings.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        self.bindings.push();
        self.bindings.bind_inputs(&self.aliases, &node.sig.inputs);
        syn::visit::visit_signature(self, &node.sig);
        self.visit_block(&node.block);
        self.bindings.pop();
    }

    fn visit_trait_item_fn(&mut self, node: &'ast TraitItemFn) {
        self.bindings.push();
        self.bindings.bind_inputs(&self.aliases, &node.sig.inputs);
        syn::visit::visit_signature(self, &node.sig);
        if let Some(block) = &node.default {
            self.visit_block(block);
        }
        self.bindings.pop();
    }

    fn visit_expr_closure(&mut self, node: &'ast ExprClosure) {
        self.bindings.push();
        for input in &node.inputs {
            self.bindings.bind_pat(&self.aliases, input, false);
        }
        if let ReturnType::Type(_, ty) = &node.output {
            self.visit_type(ty);
        }
        self.visit_expr(&node.body);
        self.bindings.pop();
    }

    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        for import in imports_from_use(node) {
            if import.glob {
                self.inspect_glob(import.canonical);
            } else {
                self.inspect_path(import.canonical);
            }
        }
    }

    fn visit_item_extern_crate(&mut self, node: &'ast ItemExternCrate) {
        self.inspect_path(import_from_extern_crate(node).canonical);
    }

    fn visit_path(&mut self, node: &'ast Path) {
        self.inspect_path(path_segments(node));
        syn::visit::visit_path(self, node);
    }

    fn visit_expr_field(&mut self, node: &'ast ExprField) {
        if let Member::Named(member) = &node.member {
            self.inspect_member(&member.to_string());
        }
        syn::visit::visit_expr_field(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        self.inspect_method(
            &node.method.to_string(),
            self.bindings
                .receiver_is_provider(&self.aliases, &node.receiver),
        );
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_field_pat(&mut self, node: &'ast FieldPat) {
        if let Member::Named(member) = &node.member {
            self.inspect_member(&member.to_string());
        }
        syn::visit::visit_field_pat(self, node);
    }

    fn visit_field_value(&mut self, node: &'ast FieldValue) {
        if let Member::Named(member) = &node.member {
            self.inspect_member(&member.to_string());
        }
        syn::visit::visit_field_value(self, node);
    }

    fn visit_macro(&mut self, node: &'ast Macro) {
        self.inspect_macro(node);
    }
}

fn path_segments(path: &Path) -> Vec<String> {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect()
}

fn path_has_prefix(path: &[String], prefix: &[&str]) -> bool {
    path.len() >= prefix.len()
        && path
            .iter()
            .zip(prefix)
            .all(|(segment, expected)| segment == expected)
}

fn is_provider_type_name(name: &str) -> bool {
    PROVIDER_TYPES.contains(&name) || CONCRETE_PROVIDER_TYPES.contains(&name)
}

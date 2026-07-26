mod aliases;
mod bindings;
mod cfg;
mod flow;
mod modules;
mod providers;
mod tokens;

use std::collections::BTreeMap;

use aliases::{AliasStack, import_from_extern_crate, imports_from_use};
use bindings::{ProviderBindings, ProviderShape};
pub(super) use modules::external_modules;
use providers::{
    LOW_COLLISION_PROVIDER_METHODS, PROVIDER_GLOB_ROOTS, PROVIDER_HANDLES, is_provider_type_name,
};
use syn::visit::Visit;
use syn::{
    Arm, Block, ExprAssign, ExprAsync, ExprBinary, ExprClosure, ExprField, ExprForLoop, ExprIf,
    ExprLoop, ExprMatch, ExprMethodCall, ExprWhile, FieldPat, FieldValue, ImplItem, ImplItemFn,
    Item, ItemExternCrate, ItemFn, ItemMod, ItemUse, Local, Macro, Member, ReturnType, TraitItem,
    TraitItemFn,
};
use tokens::TokenFinding;

use super::{Finding, ReachRule};

pub(super) fn scan(syntax: &syn::File, rel: &str, reach_rules: &[ReachRule]) -> Vec<Finding> {
    let mut scanner = Scanner {
        rel,
        reach_rules,
        aliases: AliasStack::default(),
        bindings: ProviderBindings::default(),
        block_result_shapes: BTreeMap::new(),
        findings: Vec::new(),
    };
    scanner.visit_file(syntax);
    scanner.findings
}

struct Scanner<'a> {
    rel: &'a str,
    reach_rules: &'a [ReachRule],
    aliases: AliasStack,
    bindings: ProviderBindings,
    block_result_shapes: BTreeMap<usize, ProviderShape>,
    findings: Vec<Finding>,
}

impl Scanner<'_> {
    fn expr_shape(&self, expr: &syn::Expr) -> ProviderShape {
        match expr {
            syn::Expr::Block(block) => self
                .block_result_shapes
                .get(&block_key(&block.block))
                .cloned()
                .unwrap_or_else(|| self.bindings.expr_shape(&self.aliases, expr)),
            syn::Expr::If(branch) => {
                let then_shape = self
                    .block_result_shapes
                    .get(&block_key(&branch.then_branch))
                    .cloned()
                    .unwrap_or(ProviderShape::Scalar(false));
                let else_shape = branch
                    .else_branch
                    .as_ref()
                    .map_or(ProviderShape::Scalar(false), |(_, otherwise)| {
                        self.expr_shape(otherwise)
                    });
                ProviderShape::merge(&then_shape, &else_shape)
            }
            syn::Expr::Match(branch) => branch
                .arms
                .iter()
                .map(|arm| self.expr_shape(&arm.body))
                .reduce(|left, right| ProviderShape::merge(&left, &right))
                .unwrap_or(ProviderShape::Scalar(false)),
            _ => self.bindings.expr_shape(&self.aliases, expr),
        }
    }

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
        self.bindings.push_items(&self.aliases, &node.items);
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
            let shape = node
                .init
                .as_ref()
                .map_or(ProviderShape::Scalar(false), |init| {
                    self.expr_shape(&init.expr)
                });
            self.bindings
                .bind_pat_shape(&self.aliases, &node.pat, &shape);
        }
    }

    fn visit_arm(&mut self, node: &'ast Arm) {
        if !cfg::attrs_are_test_only(&node.attrs) {
            flow::visit_arm(self, node, &ProviderShape::Scalar(false));
        }
    }

    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        if let Some((_, items)) = &node.content {
            self.aliases.push_items(items);
            self.bindings.push_items(&self.aliases, items);
            for item in items {
                self.visit_item(item);
            }
            self.bindings.pop();
            self.aliases.pop();
        }
    }

    fn visit_block(&mut self, node: &'ast Block) {
        self.aliases.push_stmts(&node.stmts);
        let items: Vec<_> = node
            .stmts
            .iter()
            .filter_map(|stmt| match stmt {
                syn::Stmt::Item(item) => Some(item.clone()),
                _ => None,
            })
            .collect();
        self.bindings.push_items(&self.aliases, &items);
        for stmt in &node.stmts {
            self.visit_stmt(stmt);
        }
        let result_shape = match node.stmts.last() {
            Some(syn::Stmt::Expr(expr, None)) => self.expr_shape(expr),
            _ => ProviderShape::Scalar(false),
        };
        self.block_result_shapes
            .insert(block_key(node), result_shape);
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
        let entry = self.bindings.checkpoint();
        self.bindings.push();
        for input in &node.inputs {
            self.bindings
                .bind_pat_shape(&self.aliases, input, &ProviderShape::Scalar(false));
        }
        if let ReturnType::Type(_, ty) = &node.output {
            self.visit_type(ty);
        }
        self.visit_expr(&node.body);
        self.bindings.pop();
        let body_exit = self.bindings.checkpoint();
        self.bindings.merge_control_flow(&[entry, body_exit]);
    }

    fn visit_expr_assign(&mut self, node: &'ast ExprAssign) {
        flow::visit_assign(self, node);
    }

    fn visit_expr_async(&mut self, node: &'ast ExprAsync) {
        let entry = self.bindings.checkpoint();
        syn::visit::visit_expr_async(self, node);
        let body_exit = self.bindings.checkpoint();
        self.bindings.merge_control_flow(&[entry, body_exit]);
    }

    fn visit_expr_binary(&mut self, node: &'ast ExprBinary) {
        flow::visit_binary(self, node);
    }

    fn visit_expr_if(&mut self, node: &'ast ExprIf) {
        flow::visit_if(self, node);
    }

    fn visit_expr_while(&mut self, node: &'ast ExprWhile) {
        flow::visit_while(self, node);
    }

    fn visit_expr_for_loop(&mut self, node: &'ast ExprForLoop) {
        flow::visit_for(self, node);
    }

    fn visit_expr_loop(&mut self, node: &'ast ExprLoop) {
        flow::visit_loop(self, node);
    }

    fn visit_expr_match(&mut self, node: &'ast ExprMatch) {
        flow::visit_match(self, node);
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

    fn visit_path(&mut self, node: &'ast syn::Path) {
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

fn block_key(block: &Block) -> usize {
    std::ptr::from_ref(block).addr()
}

fn path_segments(path: &syn::Path) -> Vec<String> {
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

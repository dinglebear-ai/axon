use syn::visit::Visit;
use syn::{Arm, Expr, ExprAssign, ExprForLoop, ExprIf, ExprMatch, ExprWhile};

use super::Scanner;
use super::bindings::ProviderShape;

pub(super) fn visit_assign(scanner: &mut Scanner<'_>, node: &ExprAssign) {
    scanner.visit_expr(&node.left);
    scanner.visit_expr(&node.right);
    let shape = scanner.bindings.expr_shape(&scanner.aliases, &node.right);
    scanner.bindings.assign_expr_shape(&node.left, &shape);
}

pub(super) fn visit_if(scanner: &mut Scanner<'_>, node: &ExprIf) {
    if let Expr::Let(binding) = &*node.cond {
        scanner.visit_expr(&binding.expr);
        scanner.visit_pat(&binding.pat);
        let shape = scanner.bindings.expr_shape(&scanner.aliases, &binding.expr);
        let entry = scanner.bindings.checkpoint();
        scanner.bindings.restore(&entry);
        scanner.bindings.push();
        scanner
            .bindings
            .bind_pat_shape(&scanner.aliases, &binding.pat, &shape);
        scanner.visit_block(&node.then_branch);
        scanner.bindings.pop();
        let then_exit = scanner.bindings.checkpoint();
        scanner.bindings.restore(&entry);
        if let Some((_, otherwise)) = &node.else_branch {
            scanner.visit_expr(otherwise);
        }
        let else_exit = scanner.bindings.checkpoint();
        scanner.bindings.merge_control_flow(&[then_exit, else_exit]);
    } else {
        scanner.visit_expr(&node.cond);
        let entry = scanner.bindings.checkpoint();
        scanner.bindings.restore(&entry);
        scanner.visit_block(&node.then_branch);
        let then_exit = scanner.bindings.checkpoint();
        scanner.bindings.restore(&entry);
        if let Some((_, otherwise)) = &node.else_branch {
            scanner.visit_expr(otherwise);
        }
        let else_exit = scanner.bindings.checkpoint();
        scanner.bindings.merge_control_flow(&[then_exit, else_exit]);
    }
}

pub(super) fn visit_while(scanner: &mut Scanner<'_>, node: &ExprWhile) {
    if let Expr::Let(binding) = &*node.cond {
        scanner.visit_expr(&binding.expr);
        scanner.visit_pat(&binding.pat);
        let shape = scanner.bindings.expr_shape(&scanner.aliases, &binding.expr);
        let entry = scanner.bindings.checkpoint();
        scanner.bindings.restore(&entry);
        scanner.bindings.push();
        scanner
            .bindings
            .bind_pat_shape(&scanner.aliases, &binding.pat, &shape);
        scanner.visit_block(&node.body);
        scanner.bindings.pop();
        let body_exit = scanner.bindings.checkpoint();
        scanner.bindings.merge_control_flow(&[entry, body_exit]);
    } else {
        scanner.visit_expr(&node.cond);
        let entry = scanner.bindings.checkpoint();
        scanner.bindings.restore(&entry);
        scanner.visit_block(&node.body);
        let body_exit = scanner.bindings.checkpoint();
        scanner.bindings.merge_control_flow(&[entry, body_exit]);
    }
}

pub(super) fn visit_for(scanner: &mut Scanner<'_>, node: &ExprForLoop) {
    scanner.visit_expr(&node.expr);
    scanner.visit_pat(&node.pat);
    let shape = scanner.bindings.expr_shape(&scanner.aliases, &node.expr);
    let entry = scanner.bindings.checkpoint();
    scanner.bindings.restore(&entry);
    scanner.bindings.push();
    scanner
        .bindings
        .bind_pat_shape(&scanner.aliases, &node.pat, &shape);
    scanner.visit_block(&node.body);
    scanner.bindings.pop();
    let body_exit = scanner.bindings.checkpoint();
    scanner.bindings.merge_control_flow(&[entry, body_exit]);
}

pub(super) fn visit_match(scanner: &mut Scanner<'_>, node: &ExprMatch) {
    scanner.visit_expr(&node.expr);
    let shape = scanner.bindings.expr_shape(&scanner.aliases, &node.expr);
    let entry = scanner.bindings.checkpoint();
    let mut exits = Vec::with_capacity(node.arms.len());
    for arm in &node.arms {
        scanner.bindings.restore(&entry);
        visit_arm(scanner, arm, &shape);
        exits.push(scanner.bindings.checkpoint());
    }
    scanner.bindings.merge_control_flow(&exits);
}

pub(super) fn visit_arm(scanner: &mut Scanner<'_>, node: &Arm, shape: &ProviderShape) {
    scanner.visit_pat(&node.pat);
    scanner.bindings.push();
    scanner
        .bindings
        .bind_pat_shape(&scanner.aliases, &node.pat, shape);
    if let Some((_, guard)) = &node.guard {
        scanner.visit_expr(guard);
    }
    scanner.visit_expr(&node.body);
    scanner.bindings.pop();
}

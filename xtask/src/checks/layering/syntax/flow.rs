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
        scanner.bindings.push();
        scanner
            .bindings
            .bind_pat_shape(&scanner.aliases, &binding.pat, &shape);
        scanner.visit_block(&node.then_branch);
        scanner.bindings.pop();
        if let Some((_, otherwise)) = &node.else_branch {
            scanner.visit_expr(otherwise);
        }
    } else {
        syn::visit::visit_expr_if(scanner, node);
    }
}

pub(super) fn visit_while(scanner: &mut Scanner<'_>, node: &ExprWhile) {
    if let Expr::Let(binding) = &*node.cond {
        scanner.visit_expr(&binding.expr);
        scanner.visit_pat(&binding.pat);
        let shape = scanner.bindings.expr_shape(&scanner.aliases, &binding.expr);
        scanner.bindings.push();
        scanner
            .bindings
            .bind_pat_shape(&scanner.aliases, &binding.pat, &shape);
        scanner.visit_block(&node.body);
        scanner.bindings.pop();
    } else {
        syn::visit::visit_expr_while(scanner, node);
    }
}

pub(super) fn visit_for(scanner: &mut Scanner<'_>, node: &ExprForLoop) {
    scanner.visit_expr(&node.expr);
    scanner.visit_pat(&node.pat);
    let shape = scanner.bindings.expr_shape(&scanner.aliases, &node.expr);
    scanner.bindings.push();
    scanner
        .bindings
        .bind_pat_shape(&scanner.aliases, &node.pat, &shape);
    scanner.visit_block(&node.body);
    scanner.bindings.pop();
}

pub(super) fn visit_match(scanner: &mut Scanner<'_>, node: &ExprMatch) {
    scanner.visit_expr(&node.expr);
    let shape = scanner.bindings.expr_shape(&scanner.aliases, &node.expr);
    for arm in &node.arms {
        visit_arm(scanner, arm, &shape);
    }
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

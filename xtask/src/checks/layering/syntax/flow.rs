use syn::visit::Visit;
use syn::{
    Arm, BinOp, Expr, ExprAssign, ExprBinary, ExprForLoop, ExprIf, ExprLoop, ExprMatch, ExprWhile,
};

use super::Scanner;
use super::bindings::{ProviderBindings, ProviderShape};

pub(super) fn visit_assign(scanner: &mut Scanner<'_>, node: &ExprAssign) {
    scanner.visit_expr(&node.left);
    scanner.visit_expr(&node.right);
    let shape = scanner.bindings.expr_shape(&scanner.aliases, &node.right);
    scanner.bindings.assign_expr_shape(&node.left, &shape);
}

pub(super) fn visit_binary(scanner: &mut Scanner<'_>, node: &ExprBinary) {
    if matches!(node.op, BinOp::And(_) | BinOp::Or(_)) {
        scanner.visit_expr(&node.left);
        let skipped_rhs = scanner.bindings.checkpoint();
        scanner.visit_expr(&node.right);
        let executed_rhs = scanner.bindings.checkpoint();
        scanner
            .bindings
            .merge_control_flow(&[skipped_rhs, executed_rhs]);
    } else {
        syn::visit::visit_expr_binary(scanner, node);
    }
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
        stabilize_and_visit_loop(scanner, &entry, |scanner| {
            scanner.bindings.push();
            scanner
                .bindings
                .bind_pat_shape(&scanner.aliases, &binding.pat, &shape);
            scanner.visit_block(&node.body);
            scanner.bindings.pop();
        });
    } else {
        scanner.visit_expr(&node.cond);
        let entry = scanner.bindings.checkpoint();
        stabilize_and_visit_loop(scanner, &entry, |scanner| scanner.visit_block(&node.body));
    }
}

pub(super) fn visit_for(scanner: &mut Scanner<'_>, node: &ExprForLoop) {
    scanner.visit_expr(&node.expr);
    scanner.visit_pat(&node.pat);
    let shape = scanner.bindings.expr_shape(&scanner.aliases, &node.expr);
    let entry = scanner.bindings.checkpoint();
    stabilize_and_visit_loop(scanner, &entry, |scanner| {
        scanner.bindings.push();
        scanner
            .bindings
            .bind_pat_shape(&scanner.aliases, &node.pat, &shape);
        scanner.visit_block(&node.body);
        scanner.bindings.pop();
    });
}

pub(super) fn visit_loop(scanner: &mut Scanner<'_>, node: &ExprLoop) {
    let entry = scanner.bindings.checkpoint();
    stabilize_and_visit_loop(scanner, &entry, |scanner| scanner.visit_block(&node.body));
}

pub(super) fn visit_match(scanner: &mut Scanner<'_>, node: &ExprMatch) {
    scanner.visit_expr(&node.expr);
    let shape = scanner.bindings.expr_shape(&scanner.aliases, &node.expr);
    let entry = scanner.bindings.checkpoint();
    let mut exits = Vec::with_capacity(node.arms.len());
    let mut arm_entry = entry;
    for arm in &node.arms {
        scanner.bindings.restore(&arm_entry);
        let (body_exit, guard_false_exit) = visit_match_arm(scanner, arm, &shape);
        exits.push(body_exit);
        if let Some(guard_false_exit) = guard_false_exit {
            scanner
                .bindings
                .merge_control_flow(&[arm_entry, guard_false_exit]);
            arm_entry = scanner.bindings.checkpoint();
        }
    }
    scanner.bindings.merge_control_flow(&exits);
}

pub(super) fn visit_arm(scanner: &mut Scanner<'_>, node: &Arm, shape: &ProviderShape) {
    let _ = visit_match_arm(scanner, node, shape);
}

fn visit_match_arm(
    scanner: &mut Scanner<'_>,
    node: &Arm,
    shape: &ProviderShape,
) -> (ProviderBindings, Option<ProviderBindings>) {
    scanner.visit_pat(&node.pat);
    scanner.bindings.push();
    scanner
        .bindings
        .bind_pat_shape(&scanner.aliases, &node.pat, shape);
    let guard_false_exit = if let Some((_, guard)) = &node.guard {
        scanner.visit_expr(guard);
        let guard_exit = scanner.bindings.checkpoint();
        scanner.bindings.pop();
        let guard_false_exit = scanner.bindings.checkpoint();
        scanner.bindings.restore(&guard_exit);
        Some(guard_false_exit)
    } else {
        None
    };
    scanner.visit_expr(&node.body);
    scanner.bindings.pop();
    (scanner.bindings.checkpoint(), guard_false_exit)
}

fn stabilize_and_visit_loop(
    scanner: &mut Scanner<'_>,
    entry: &ProviderBindings,
    mut visit_body: impl FnMut(&mut Scanner<'_>),
) {
    let mut head = entry.clone();
    loop {
        scanner.bindings.restore(&head);
        let finding_count = scanner.findings.len();
        scanner.bindings.begin_conservative_assignments();
        visit_body(scanner);
        scanner.bindings.end_conservative_assignments();
        let body_exit = scanner.bindings.checkpoint();
        scanner.findings.truncate(finding_count);
        scanner
            .bindings
            .merge_control_flow(&[head.clone(), body_exit]);
        let next_head = scanner.bindings.checkpoint();
        if next_head == head {
            break;
        }
        head = next_head;
    }

    scanner.bindings.restore(&head);
    scanner.bindings.begin_conservative_assignments();
    visit_body(scanner);
    scanner.bindings.end_conservative_assignments();
    let body_exit = scanner.bindings.checkpoint();
    scanner
        .bindings
        .merge_control_flow(&[entry.clone(), body_exit]);
}

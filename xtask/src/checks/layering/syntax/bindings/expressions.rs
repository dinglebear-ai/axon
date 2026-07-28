use syn::{Expr, Member, UnOp};

use super::{
    AliasStack, PROVIDER_HANDLES, ProviderBindings, ProviderShape, merge_shapes, path_segments,
};

impl ProviderBindings {
    pub fn expr_shape(&self, aliases: &AliasStack, expr: &Expr) -> ProviderShape {
        match expr {
            Expr::Tuple(tuple) => ProviderShape::Tuple(
                tuple
                    .elems
                    .iter()
                    .map(|expr| self.expr_shape(aliases, expr))
                    .collect(),
            ),
            Expr::Path(path) if path.path.segments.len() == 1 => self
                .value_shape(&path.path.segments[0].ident.to_string())
                .cloned()
                .unwrap_or_else(|| ProviderShape::Scalar(self.expr_is_provider(aliases, expr))),
            Expr::Block(block) => self.block_result_shape(aliases, &block.block),
            Expr::If(branch) => {
                let then_shape = self.block_result_shape(aliases, &branch.then_branch);
                let else_shape = branch
                    .else_branch
                    .as_ref()
                    .map_or(ProviderShape::Scalar(false), |(_, otherwise)| {
                        self.expr_shape(aliases, otherwise)
                    });
                merge_shapes(&then_shape, &else_shape)
            }
            Expr::Match(branch) => branch
                .arms
                .iter()
                .map(|arm| self.expr_shape(aliases, &arm.body))
                .reduce(|left, right| merge_shapes(&left, &right))
                .unwrap_or(ProviderShape::Scalar(false)),
            _ => ProviderShape::Scalar(self.expr_is_provider(aliases, expr)),
        }
    }

    pub fn expr_is_provider(&self, aliases: &AliasStack, expr: &Expr) -> bool {
        match expr {
            Expr::Path(path) => {
                let segments = path_segments(&path.path);
                (segments.len() == 1 && self.is_provider(&segments[0]))
                    || self.path_names_provider(aliases, &segments)
            }
            Expr::Field(field) => {
                matches!(&field.member, Member::Named(member)
                    if PROVIDER_HANDLES.contains(&member.to_string().as_str()))
            }
            Expr::Call(call) => {
                self.call_constructs_provider(aliases, &call.func)
                    || wrapper_propagates_provider(&call.func)
                        && call
                            .args
                            .iter()
                            .any(|argument| self.expr_is_provider(aliases, argument))
            }
            Expr::MethodCall(call)
                if matches!(
                    call.method.to_string().as_str(),
                    "clone" | "to_owned" | "as_ref"
                ) =>
            {
                self.wrapper_method_source_is_provider(aliases, &call.receiver)
            }
            Expr::Reference(reference) => self.expr_is_provider(aliases, &reference.expr),
            Expr::Paren(paren) => self.expr_is_provider(aliases, &paren.expr),
            Expr::Group(group) => self.expr_is_provider(aliases, &group.expr),
            Expr::Try(value) => self.expr_is_provider(aliases, &value.expr),
            Expr::Await(value) => self.expr_is_provider(aliases, &value.base),
            Expr::Cast(cast) => {
                self.expr_is_provider(aliases, &cast.expr)
                    || self.type_is_provider(aliases, &cast.ty)
            }
            Expr::Struct(value) => self.path_names_provider(aliases, &path_segments(&value.path)),
            Expr::Unary(unary) if matches!(unary.op, UnOp::Deref(_)) => {
                self.expr_is_provider(aliases, &unary.expr)
            }
            Expr::Index(index) => self.expr_is_provider(aliases, &index.expr),
            Expr::Block(_) | Expr::If(_) | Expr::Match(_) => {
                self.expr_shape(aliases, expr).is_provider()
            }
            _ => false,
        }
    }

    fn wrapper_method_source_is_provider(&self, aliases: &AliasStack, expr: &Expr) -> bool {
        match expr {
            Expr::Path(_)
            | Expr::Reference(_)
            | Expr::Paren(_)
            | Expr::Group(_)
            | Expr::Unary(_)
            | Expr::Index(_) => self.receiver_is_provider(aliases, expr),
            Expr::Call(call) if wrapper_propagates_provider(&call.func) => {
                self.receiver_is_provider(aliases, expr)
            }
            Expr::MethodCall(call)
                if matches!(
                    call.method.to_string().as_str(),
                    "clone" | "to_owned" | "as_ref"
                ) =>
            {
                self.wrapper_method_source_is_provider(aliases, &call.receiver)
            }
            _ => false,
        }
    }

    pub fn receiver_is_provider(&self, aliases: &AliasStack, expr: &Expr) -> bool {
        match expr {
            Expr::Path(path) => {
                let segments = path_segments(&path.path);
                (segments.len() == 1 && self.is_provider(&segments[0]))
                    || self.path_names_provider(aliases, &segments)
            }
            Expr::Field(field) => {
                matches!(&field.member, Member::Named(member)
                    if PROVIDER_HANDLES.contains(&member.to_string().as_str()))
            }
            Expr::Call(call) => {
                self.call_constructs_provider(aliases, &call.func)
                    && call_operation_is_constructor(&call.func)
                    || wrapper_propagates_provider(&call.func)
                        && call
                            .args
                            .iter()
                            .any(|argument| self.expr_is_provider(aliases, argument))
            }
            Expr::MethodCall(call)
                if matches!(
                    call.method.to_string().as_str(),
                    "clone" | "to_owned" | "as_ref"
                ) =>
            {
                self.wrapper_method_source_is_provider(aliases, &call.receiver)
            }
            Expr::Unary(unary) if matches!(unary.op, UnOp::Deref(_)) => {
                self.receiver_is_provider(aliases, &unary.expr)
            }
            Expr::Index(index) => self.receiver_is_provider(aliases, &index.expr),
            Expr::Reference(reference) => self.receiver_is_provider(aliases, &reference.expr),
            Expr::Paren(paren) => self.receiver_is_provider(aliases, &paren.expr),
            Expr::Group(group) => self.receiver_is_provider(aliases, &group.expr),
            _ => false,
        }
    }
}

fn wrapper_propagates_provider(expr: &Expr) -> bool {
    let Expr::Path(path) = expr else {
        return false;
    };
    let segments = path_segments(&path.path);
    let Some(operation) = segments.last().map(String::as_str) else {
        return false;
    };
    let Some(wrapper) = segments
        .get(segments.len().saturating_sub(2))
        .map(String::as_str)
    else {
        return false;
    };
    matches!(wrapper, "Arc" | "Box" | "Rc")
        && matches!(operation, "new" | "clone" | "as_ref" | "from")
}

fn call_operation_is_constructor(expr: &Expr) -> bool {
    let Expr::Path(path) = expr else {
        return false;
    };
    matches!(
        path.path.segments.last().map(|segment| segment.ident.to_string()),
        Some(operation)
            if matches!(
                operation.as_str(),
                "new" | "connect" | "from_config" | "from_env" | "open"
            )
    )
}

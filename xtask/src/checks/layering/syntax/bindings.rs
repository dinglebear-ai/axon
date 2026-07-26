use std::collections::BTreeMap;

use syn::{Expr, FnArg, GenericArgument, Item, Member, Pat, PathArguments, Type, TypeParamBound};

use super::aliases::AliasStack;
use super::path_segments;
use super::providers::{PROVIDER_HANDLES, is_provider_path};

#[derive(Clone, Debug)]
pub(super) enum ProviderShape {
    Scalar(bool),
    Tuple(Vec<Self>),
}

impl ProviderShape {
    fn is_provider(&self) -> bool {
        match self {
            Self::Scalar(provider) => *provider,
            Self::Tuple(elements) => elements.iter().any(Self::is_provider),
        }
    }
}

#[derive(Default)]
struct BindingScope {
    values: BTreeMap<String, ProviderShape>,
    type_aliases: BTreeMap<String, bool>,
}

#[derive(Default)]
pub(super) struct ProviderBindings {
    scopes: Vec<BindingScope>,
}

impl ProviderBindings {
    pub fn push_items(&mut self, aliases: &AliasStack, items: &[Item]) {
        self.scopes.push(BindingScope::default());
        let alias_types: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                Item::Type(alias) => Some((alias.ident.to_string(), &*alias.ty)),
                _ => None,
            })
            .collect();
        for (name, _) in &alias_types {
            self.bind_type_alias(name.clone(), false);
        }
        for _ in 0..alias_types.len() {
            let mut changed = false;
            for (name, ty) in &alias_types {
                let provider = self.type_is_provider(aliases, ty);
                changed |= self.bind_type_alias(name.clone(), provider);
            }
            if !changed {
                break;
            }
        }
    }

    pub fn push(&mut self) {
        self.scopes.push(BindingScope::default());
    }

    pub fn pop(&mut self) {
        self.scopes.pop();
    }

    pub fn bind(&mut self, name: String, shape: ProviderShape) {
        self.scopes
            .last_mut()
            .expect("provider binding scope must exist")
            .values
            .insert(name, shape);
    }

    pub fn assign(&mut self, name: &str, shape: ProviderShape) {
        if let Some(scope) = self
            .scopes
            .iter_mut()
            .rev()
            .find(|scope| scope.values.contains_key(name))
        {
            scope.values.insert(name.to_owned(), shape);
        }
    }

    pub fn is_provider(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.values.get(name))
            .is_some_and(ProviderShape::is_provider)
    }

    pub fn type_shape(&self, aliases: &AliasStack, ty: &Type) -> ProviderShape {
        match ty {
            Type::Tuple(tuple) => ProviderShape::Tuple(
                tuple
                    .elems
                    .iter()
                    .map(|ty| self.type_shape(aliases, ty))
                    .collect(),
            ),
            _ => ProviderShape::Scalar(self.type_is_provider(aliases, ty)),
        }
    }

    pub fn type_is_provider(&self, aliases: &AliasStack, ty: &Type) -> bool {
        match ty {
            Type::Path(path) => {
                let segments = path_segments(&path.path);
                self.path_names_provider(aliases, &segments)
                    || path.path.segments.iter().any(|segment| {
                        let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                            return false;
                        };
                        arguments.args.iter().any(|argument| match argument {
                            GenericArgument::Type(ty) => self.type_is_provider(aliases, ty),
                            GenericArgument::AssocType(assoc) => {
                                self.type_is_provider(aliases, &assoc.ty)
                            }
                            GenericArgument::Constraint(constraint) => {
                                constraint.bounds.iter().any(|bound| {
                                    matches!(bound, TypeParamBound::Trait(bound)
                                    if self.path_names_provider(
                                        aliases,
                                        &path_segments(&bound.path)
                                    ))
                                })
                            }
                            _ => false,
                        })
                    })
            }
            Type::Reference(reference) => self.type_is_provider(aliases, &reference.elem),
            Type::TraitObject(object) => object.bounds.iter().any(|bound| {
                matches!(bound, TypeParamBound::Trait(bound)
                    if self.path_names_provider(aliases, &path_segments(&bound.path)))
            }),
            Type::ImplTrait(object) => object.bounds.iter().any(|bound| {
                matches!(bound, TypeParamBound::Trait(bound)
                    if self.path_names_provider(aliases, &path_segments(&bound.path)))
            }),
            Type::Paren(paren) => self.type_is_provider(aliases, &paren.elem),
            Type::Group(group) => self.type_is_provider(aliases, &group.elem),
            Type::Tuple(tuple) => tuple
                .elems
                .iter()
                .any(|ty| self.type_is_provider(aliases, ty)),
            _ => false,
        }
    }

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
                self.receiver_is_provider(aliases, &call.receiver)
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
            Expr::Reference(reference) => self.receiver_is_provider(aliases, &reference.expr),
            Expr::Paren(paren) => self.receiver_is_provider(aliases, &paren.expr),
            Expr::Group(group) => self.receiver_is_provider(aliases, &group.expr),
            _ => false,
        }
    }

    pub fn bind_pat_shape(&mut self, aliases: &AliasStack, pat: &Pat, shape: &ProviderShape) {
        match pat {
            Pat::Ident(ident) => {
                self.bind(ident.ident.to_string(), shape.clone());
                if let Some((_, subpat)) = &ident.subpat {
                    self.bind_pat_shape(aliases, subpat, shape);
                }
            }
            Pat::Type(typed) => {
                let typed_shape = self.type_shape(aliases, &typed.ty);
                self.bind_pat_shape(aliases, &typed.pat, &merge_shapes(shape, &typed_shape));
            }
            Pat::Reference(reference) => self.bind_pat_shape(aliases, &reference.pat, shape),
            Pat::Paren(paren) => self.bind_pat_shape(aliases, &paren.pat, shape),
            Pat::Tuple(tuple) => self.bind_positional(aliases, &tuple.elems, shape),
            Pat::TupleStruct(tuple) => self.bind_positional(aliases, &tuple.elems, shape),
            Pat::Struct(structure) => {
                for field in &structure.fields {
                    let field_provider = shape.is_provider()
                        || matches!(&field.member, Member::Named(member)
                            if PROVIDER_HANDLES.contains(&member.to_string().as_str()));
                    self.bind_pat_shape(
                        aliases,
                        &field.pat,
                        &ProviderShape::Scalar(field_provider),
                    );
                }
            }
            Pat::Slice(slice) => self.bind_positional(aliases, &slice.elems, shape),
            Pat::Or(or) => {
                for case in &or.cases {
                    self.bind_pat_shape(aliases, case, shape);
                }
            }
            _ => {}
        }
    }

    pub fn assign_expr_shape(&mut self, expr: &Expr, shape: &ProviderShape) {
        match expr {
            Expr::Path(path) if path.path.segments.len() == 1 => {
                self.assign(&path.path.segments[0].ident.to_string(), shape.clone());
            }
            Expr::Tuple(tuple) => {
                for (index, element) in tuple.elems.iter().enumerate() {
                    self.assign_expr_shape(element, tuple_element(shape, index));
                }
            }
            Expr::Paren(paren) => self.assign_expr_shape(&paren.expr, shape),
            Expr::Group(group) => self.assign_expr_shape(&group.expr, shape),
            _ => {}
        }
    }

    pub fn bind_inputs<'a>(
        &mut self,
        aliases: &AliasStack,
        inputs: impl IntoIterator<Item = &'a FnArg>,
    ) {
        for input in inputs {
            if let FnArg::Typed(argument) = input {
                let shape = self.type_shape(aliases, &argument.ty);
                self.bind_pat_shape(aliases, &argument.pat, &shape);
            }
        }
    }

    fn bind_positional(
        &mut self,
        aliases: &AliasStack,
        patterns: &syn::punctuated::Punctuated<Pat, syn::token::Comma>,
        shape: &ProviderShape,
    ) {
        for (index, pattern) in patterns.iter().enumerate() {
            self.bind_pat_shape(aliases, pattern, tuple_element(shape, index));
        }
    }

    fn bind_type_alias(&mut self, name: String, provider: bool) -> bool {
        let previous = self
            .scopes
            .last_mut()
            .expect("provider binding scope must exist")
            .type_aliases
            .insert(name, provider);
        previous != Some(provider)
    }

    fn path_names_provider(&self, aliases: &AliasStack, path: &[String]) -> bool {
        let resolved = aliases.resolve(path);
        (resolved.len() == 1 && self.type_alias_is_provider(&resolved[0]))
            || is_provider_path(&resolved)
    }

    fn call_constructs_provider(&self, aliases: &AliasStack, callable: &Expr) -> bool {
        let Expr::Path(path) = callable else {
            return false;
        };
        let mut owner = path_segments(&path.path);
        owner.pop();
        !owner.is_empty() && self.path_names_provider(aliases, &owner)
    }

    fn type_alias_is_provider(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.type_aliases.get(name))
            .copied()
            .unwrap_or(false)
    }

    fn value_shape(&self, name: &str) -> Option<&ProviderShape> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.values.get(name))
    }
}

fn tuple_element(shape: &ProviderShape, index: usize) -> &ProviderShape {
    static NOT_PROVIDER: ProviderShape = ProviderShape::Scalar(false);
    match shape {
        ProviderShape::Tuple(elements) => elements.get(index).unwrap_or(&NOT_PROVIDER),
        ProviderShape::Scalar(_) => shape,
    }
}

fn merge_shapes(left: &ProviderShape, right: &ProviderShape) -> ProviderShape {
    match (left, right) {
        (ProviderShape::Tuple(left), ProviderShape::Tuple(right)) => ProviderShape::Tuple(
            left.iter()
                .zip(right)
                .map(|(left, right)| merge_shapes(left, right))
                .collect(),
        ),
        _ => ProviderShape::Scalar(left.is_provider() || right.is_provider()),
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

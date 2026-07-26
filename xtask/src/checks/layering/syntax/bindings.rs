use std::collections::BTreeMap;

use syn::{Expr, FnArg, GenericArgument, Member, Pat, PathArguments, Type, TypeParamBound};

use super::aliases::AliasStack;
use super::{PROVIDER_HANDLES, is_provider_type_name, path_segments};

#[derive(Default)]
pub(super) struct ProviderBindings {
    scopes: Vec<BTreeMap<String, bool>>,
}

impl ProviderBindings {
    pub fn push(&mut self) {
        self.scopes.push(BTreeMap::new());
    }

    pub fn pop(&mut self) {
        self.scopes.pop();
    }

    pub fn bind(&mut self, name: String, is_provider: bool) {
        self.scopes
            .last_mut()
            .expect("provider binding scope must exist")
            .insert(name, is_provider);
    }

    pub fn is_provider(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .copied()
            .unwrap_or(false)
    }

    pub fn type_is_provider(&self, aliases: &AliasStack, ty: &Type) -> bool {
        match ty {
            Type::Path(path) => {
                let segments = path_segments(&path.path);
                path_names_provider(aliases, &segments)
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
                                        if path_names_provider(aliases, &path_segments(&bound.path)))
                                })
                            }
                            _ => false,
                        })
                    })
            }
            Type::Reference(reference) => self.type_is_provider(aliases, &reference.elem),
            Type::TraitObject(object) => object.bounds.iter().any(|bound| {
                matches!(bound, TypeParamBound::Trait(bound)
                    if path_names_provider(aliases, &path_segments(&bound.path)))
            }),
            Type::ImplTrait(object) => object.bounds.iter().any(|bound| {
                matches!(bound, TypeParamBound::Trait(bound)
                    if path_names_provider(aliases, &path_segments(&bound.path)))
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

    pub fn expr_is_provider(&self, aliases: &AliasStack, expr: &Expr) -> bool {
        match expr {
            Expr::Path(path) => {
                let segments = path_segments(&path.path);
                (segments.len() == 1 && self.is_provider(&segments[0]))
                    || path_names_provider(aliases, &segments)
            }
            Expr::Field(field) => {
                matches!(&field.member, Member::Named(member)
                    if PROVIDER_HANDLES.contains(&member.to_string().as_str()))
            }
            Expr::Call(call) => {
                self.expr_is_provider(aliases, &call.func)
                    || is_provider_wrapper(&call.func)
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
            Expr::Struct(value) => path_names_provider(aliases, &path_segments(&value.path)),
            _ => false,
        }
    }

    pub fn receiver_is_provider(&self, aliases: &AliasStack, expr: &Expr) -> bool {
        match expr {
            Expr::Path(path) => {
                let segments = path_segments(&path.path);
                (segments.len() == 1 && self.is_provider(&segments[0]))
                    || path_names_provider(aliases, &segments)
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

    pub fn bind_pat(&mut self, aliases: &AliasStack, pat: &Pat, is_provider: bool) {
        match pat {
            Pat::Ident(ident) => {
                self.bind(ident.ident.to_string(), is_provider);
                if let Some((_, subpat)) = &ident.subpat {
                    self.bind_pat(aliases, subpat, is_provider);
                }
            }
            Pat::Type(typed) => {
                let provider = is_provider || self.type_is_provider(aliases, &typed.ty);
                self.bind_pat(aliases, &typed.pat, provider);
            }
            Pat::Reference(reference) => self.bind_pat(aliases, &reference.pat, is_provider),
            Pat::Paren(paren) => self.bind_pat(aliases, &paren.pat, is_provider),
            Pat::Tuple(tuple) => {
                for element in &tuple.elems {
                    self.bind_pat(aliases, element, is_provider);
                }
            }
            Pat::TupleStruct(tuple) => {
                for element in &tuple.elems {
                    self.bind_pat(aliases, element, is_provider);
                }
            }
            Pat::Struct(structure) => {
                for field in &structure.fields {
                    let field_is_provider = is_provider
                        || matches!(&field.member, Member::Named(member)
                            if PROVIDER_HANDLES.contains(&member.to_string().as_str()));
                    self.bind_pat(aliases, &field.pat, field_is_provider);
                }
            }
            Pat::Slice(slice) => {
                for element in &slice.elems {
                    self.bind_pat(aliases, element, is_provider);
                }
            }
            Pat::Or(or) => {
                for case in &or.cases {
                    self.bind_pat(aliases, case, is_provider);
                }
            }
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
                let provider = self.type_is_provider(aliases, &argument.ty);
                self.bind_pat(aliases, &argument.pat, provider);
            }
        }
    }
}

fn path_names_provider(aliases: &AliasStack, path: &[String]) -> bool {
    aliases
        .resolve(path)
        .iter()
        .any(|name| is_provider_type_name(name))
}

fn is_provider_wrapper(expr: &Expr) -> bool {
    let Expr::Path(path) = expr else {
        return false;
    };
    let segments = path_segments(&path.path);
    matches!(
        segments.as_slice(),
        [wrapper, constructor]
            if matches!(wrapper.as_str(), "Arc" | "Box" | "Rc")
                && constructor == "new"
    )
}

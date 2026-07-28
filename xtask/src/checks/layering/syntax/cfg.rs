use syn::punctuated::Punctuated;
use syn::{Attribute, ImplItem, Item, Meta, Token, TraitItem};

pub(super) fn attrs_are_test_only(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attribute| {
        attribute.path().is_ident("cfg")
            && attribute
                .parse_args::<Meta>()
                .is_ok_and(|meta| meta_implies_test(&meta))
    })
}

pub(super) fn item_is_test_only(item: &Item) -> bool {
    item_attrs(item).is_some_and(attrs_are_test_only)
}

pub(super) fn impl_item_is_test_only(item: &ImplItem) -> bool {
    match item {
        ImplItem::Const(item) => attrs_are_test_only(&item.attrs),
        ImplItem::Fn(item) => attrs_are_test_only(&item.attrs),
        ImplItem::Type(item) => attrs_are_test_only(&item.attrs),
        ImplItem::Macro(item) => attrs_are_test_only(&item.attrs),
        _ => false,
    }
}

pub(super) fn trait_item_is_test_only(item: &TraitItem) -> bool {
    match item {
        TraitItem::Const(item) => attrs_are_test_only(&item.attrs),
        TraitItem::Fn(item) => attrs_are_test_only(&item.attrs),
        TraitItem::Type(item) => attrs_are_test_only(&item.attrs),
        TraitItem::Macro(item) => attrs_are_test_only(&item.attrs),
        _ => false,
    }
}

fn meta_implies_test(meta: &Meta) -> bool {
    match meta {
        Meta::Path(path) => path.is_ident("test"),
        Meta::List(list) if list.path.is_ident("all") => {
            parse_nested(list).is_some_and(|nested| nested.iter().any(meta_implies_test))
        }
        Meta::List(list) if list.path.is_ident("any") => parse_nested(list)
            .is_some_and(|nested| !nested.is_empty() && nested.iter().all(meta_implies_test)),
        _ => false,
    }
}

fn parse_nested(list: &syn::MetaList) -> Option<Punctuated<Meta, Token![,]>> {
    list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .ok()
}

fn item_attrs(item: &Item) -> Option<&[Attribute]> {
    match item {
        Item::Const(item) => Some(&item.attrs),
        Item::Enum(item) => Some(&item.attrs),
        Item::ExternCrate(item) => Some(&item.attrs),
        Item::Fn(item) => Some(&item.attrs),
        Item::ForeignMod(item) => Some(&item.attrs),
        Item::Impl(item) => Some(&item.attrs),
        Item::Macro(item) => Some(&item.attrs),
        Item::Mod(item) => Some(&item.attrs),
        Item::Static(item) => Some(&item.attrs),
        Item::Struct(item) => Some(&item.attrs),
        Item::Trait(item) => Some(&item.attrs),
        Item::TraitAlias(item) => Some(&item.attrs),
        Item::Type(item) => Some(&item.attrs),
        Item::Union(item) => Some(&item.attrs),
        Item::Use(item) => Some(&item.attrs),
        _ => None,
    }
}

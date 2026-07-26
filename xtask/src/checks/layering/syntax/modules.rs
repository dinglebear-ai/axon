use std::path::{Path, PathBuf};

use syn::Item;

use super::cfg;

pub(crate) struct ExternalModule {
    pub path: PathBuf,
    pub test_only: bool,
}

pub(crate) fn external_modules(syntax: &syn::File, source: &Path) -> Vec<ExternalModule> {
    let parent = source.parent().unwrap_or_else(|| Path::new(""));
    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let module_dir = if matches!(stem, "lib" | "main") {
        parent.to_owned()
    } else {
        parent.join(stem)
    };
    let mut modules = Vec::new();
    collect(&syntax.items, &module_dir, parent, false, &mut modules);
    modules
}

fn collect(
    items: &[Item],
    module_dir: &Path,
    path_attr_base: &Path,
    inherited_test_only: bool,
    output: &mut Vec<ExternalModule>,
) {
    for item in items {
        let Item::Mod(module) = item else {
            continue;
        };
        let test_only = inherited_test_only || cfg::item_is_test_only(item);
        let path_override = module.attrs.iter().find_map(path_override);
        if let Some((_, nested)) = &module.content {
            let nested_dir = path_override.as_ref().map_or_else(
                || module_dir.join(module.ident.to_string()),
                |path| path_attr_base.join(path),
            );
            collect(nested, &nested_dir, &nested_dir, test_only, output);
        } else {
            let path = path_override.map_or_else(
                || module_dir.join(format!("{}.rs", module.ident)),
                |path| path_attr_base.join(path),
            );
            output.push(ExternalModule {
                path: normalize(&path),
                test_only,
            });
        }
    }
}

fn path_override(attribute: &syn::Attribute) -> Option<PathBuf> {
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
    Some(PathBuf::from(path.value()))
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {}
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

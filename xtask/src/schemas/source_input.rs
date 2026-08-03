use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, Item, ItemMod, Lit, LitStr, Meta};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceInput {
    pub path: String,
    pub kind: SourceInputKind,
    pub checksum: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceInputKind {
    RustModule,
    RustDirectory,
    MarkdownContract,
    SqlMigrationDirectory,
}

pub fn source_inputs(root: &Path, paths: &[&str]) -> Result<Vec<SourceInput>> {
    let mut cache = SourceInputCache::default();
    source_inputs_with_cache(root, paths, &mut cache)
}

pub fn source_inputs_with_cache(
    root: &Path,
    paths: &[&str],
    cache: &mut SourceInputCache,
) -> Result<Vec<SourceInput>> {
    let mut inputs = Vec::with_capacity(paths.len());
    for path in paths {
        inputs.push(cache.source_input(root, PathBuf::from(path))?);
    }
    inputs.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(inputs)
}

/// Build source provenance from explicit inputs plus the transitive Rust source
/// closure rooted at `rust_roots`.
///
/// The closure follows out-of-line `mod` declarations, literal `#[path]`
/// modules, and literal `include!` files. Missing or unparsable referenced Rust
/// sources fail closed. Items gated by an exact `cfg(test)` are deliberately
/// excluded from production contract provenance.
pub(super) fn source_inputs_with_rust_module_closure(
    root: &Path,
    explicit_paths: &[&str],
    rust_roots: &[&str],
) -> Result<Vec<SourceInput>> {
    let mut paths = explicit_paths
        .iter()
        .map(|path| clean_relative_path(Path::new(path)))
        .collect::<Result<BTreeSet<_>>>()?;
    paths.extend(rust_module_closure(root, rust_roots)?);

    let mut cache = SourceInputCache::default();
    let mut inputs = paths
        .into_iter()
        .map(|path| cache.source_input(root, path))
        .collect::<Result<Vec<_>>>()?;
    inputs.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(inputs)
}

fn rust_module_closure(root: &Path, rust_roots: &[&str]) -> Result<BTreeSet<PathBuf>> {
    let mut pending = rust_roots
        .iter()
        .map(|path| clean_relative_path(Path::new(path)))
        .collect::<Result<VecDeque<_>>>()?;
    let mut visited = BTreeSet::new();

    while let Some(rel_path) = pending.pop_front() {
        if !visited.insert(rel_path.clone()) {
            continue;
        }

        let source_path = root.join(&rel_path);
        let source = std::fs::read_to_string(&source_path).with_context(|| {
            format!(
                "failed to read referenced Rust schema source {}",
                rel_path.display()
            )
        })?;
        let file = syn::parse_file(&source).with_context(|| {
            format!(
                "failed to parse referenced Rust schema source {}",
                rel_path.display()
            )
        })?;
        let source_dir = rel_path.parent().unwrap_or_else(|| Path::new(""));
        let module_dir = module_directory(&rel_path)?;
        collect_item_references(
            root,
            source_dir,
            &module_dir,
            source_dir,
            &file.items,
            &mut pending,
        )?;
    }

    Ok(visited)
}

fn collect_item_references(
    root: &Path,
    source_dir: &Path,
    module_dir: &Path,
    path_attribute_base: &Path,
    items: &[Item],
    pending: &mut VecDeque<PathBuf>,
) -> Result<()> {
    for item in items {
        if item_attrs(item).is_some_and(is_cfg_test) {
            continue;
        }
        if let Item::Mod(module) = item {
            if let Some((_, items)) = &module.content {
                let inline_dir = if let Some(path) = path_attribute(&module.attrs)? {
                    clean_relative_path(&path_attribute_base.join(path))?
                } else {
                    clean_relative_path(&module_dir.join(module.ident.to_string()))?
                };
                collect_item_references(
                    root,
                    source_dir,
                    &inline_dir,
                    &inline_dir,
                    items,
                    pending,
                )?;
            } else {
                pending.push_back(resolve_module_path(
                    root,
                    path_attribute_base,
                    module_dir,
                    module,
                )?);
            }
            continue;
        }

        let mut visitor = LiteralIncludeVisitor::default();
        visitor.visit_item(item);
        for include_path in visitor.paths {
            let include_path = include_path.with_context(|| {
                format!(
                    "schema provenance requires literal include! paths in {}",
                    source_dir.display()
                )
            })?;
            let rel_path = clean_relative_path(&source_dir.join(include_path.value()))?;
            if !root.join(&rel_path).is_file() {
                bail!(
                    "referenced Rust include! source does not exist: {}",
                    rel_path.display()
                );
            }
            pending.push_back(rel_path);
        }
    }
    Ok(())
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
        Item::Verbatim(_) => None,
        _ => None,
    }
}

fn resolve_module_path(
    root: &Path,
    path_attribute_base: &Path,
    module_dir: &Path,
    module: &ItemMod,
) -> Result<PathBuf> {
    if let Some(path) = path_attribute(&module.attrs)? {
        let rel_path = clean_relative_path(&path_attribute_base.join(path))?;
        if !root.join(&rel_path).is_file() {
            bail!(
                "referenced Rust #[path] module does not exist: {}",
                rel_path.display()
            );
        }
        return Ok(rel_path);
    }

    let module_name = module.ident.to_string();
    let module_file = clean_relative_path(&module_dir.join(format!("{module_name}.rs")))?;
    let legacy_file = clean_relative_path(&module_dir.join(&module_name).join("mod.rs"))?;
    let module_exists = root.join(&module_file).is_file();
    let legacy_exists = root.join(&legacy_file).is_file();
    match (module_exists, legacy_exists) {
        (true, false) => Ok(module_file),
        (false, true) => Ok(legacy_file),
        (true, true) => bail!(
            "ambiguous Rust module {}: both {} and {} exist",
            module_name,
            module_file.display(),
            legacy_file.display()
        ),
        (false, false) => bail!(
            "referenced Rust module {} does not exist (tried {} and {})",
            module_name,
            module_file.display(),
            legacy_file.display()
        ),
    }
}

fn path_attribute(attrs: &[Attribute]) -> Result<Option<String>> {
    for attr in attrs {
        if !attr.path().is_ident("path") {
            continue;
        }
        let Meta::NameValue(name_value) = &attr.meta else {
            bail!("Rust #[path] attribute must be a string literal");
        };
        let Expr::Lit(expr) = &name_value.value else {
            bail!("Rust #[path] attribute must be a string literal");
        };
        let Lit::Str(path) = &expr.lit else {
            bail!("Rust #[path] attribute must be a string literal");
        };
        return Ok(Some(path.value()));
    }
    Ok(None)
}

fn is_cfg_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Path>()
                .is_ok_and(|path| path.is_ident("test"))
    })
}

fn module_directory(rel_path: &Path) -> Result<PathBuf> {
    let parent = rel_path.parent().unwrap_or_else(|| Path::new(""));
    let file_name = rel_path
        .file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("invalid Rust schema source path {}", rel_path.display()))?;
    if matches!(file_name, "lib.rs" | "main.rs" | "mod.rs") {
        return Ok(parent.to_path_buf());
    }
    let stem = rel_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .with_context(|| format!("invalid Rust schema source path {}", rel_path.display()))?;
    Ok(parent.join(stem))
}

fn clean_relative_path(path: &Path) -> Result<PathBuf> {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(component) => clean.push(component),
            Component::ParentDir => {
                if !clean.pop() {
                    bail!(
                        "schema source path escapes repository root: {}",
                        path.display()
                    );
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                bail!(
                    "schema source path must be repository-relative: {}",
                    path.display()
                );
            }
        }
    }
    Ok(clean)
}

#[derive(Default)]
struct LiteralIncludeVisitor {
    paths: Vec<syn::Result<LitStr>>,
}

impl<'ast> Visit<'ast> for LiteralIncludeVisitor {
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if node.path.is_ident("include") {
            self.paths.push(syn::parse2(node.tokens.clone()));
        }
        visit::visit_macro(self, node);
    }
}

#[derive(Debug, Default)]
pub struct SourceInputCache {
    checksums: BTreeMap<String, String>,
}

impl SourceInputCache {
    pub fn source_input(&mut self, root: &Path, rel_path: PathBuf) -> Result<SourceInput> {
        let path = root.join(&rel_path);
        let normalized_path = normalize_path(&rel_path);
        let is_dir = path.is_dir();
        let checksum = if let Some(checksum) = self.checksums.get(&normalized_path) {
            checksum.clone()
        } else {
            let digest = if is_dir {
                directory_digest(root, &rel_path)?
            } else {
                file_digest(&path).with_context(|| {
                    format!("failed to read schema source input {}", rel_path.display())
                })?
            };
            let checksum = format!("sha256:{digest}");
            self.checksums
                .insert(normalized_path.clone(), checksum.clone());
            checksum
        };
        Ok(SourceInput {
            kind: source_input_kind(&normalized_path, is_dir),
            path: normalized_path,
            checksum,
        })
    }
}

pub(super) fn source_input_kind(path: &str, is_dir: bool) -> SourceInputKind {
    let path = path.replace('\\', "/");
    if path.split('/').any(|component| component == "migrations") && is_dir {
        SourceInputKind::SqlMigrationDirectory
    } else if is_dir {
        SourceInputKind::RustDirectory
    } else if path.ends_with(".md") {
        SourceInputKind::MarkdownContract
    } else {
        SourceInputKind::RustModule
    }
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn file_digest(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn directory_digest(root: &Path, rel_path: &Path) -> Result<String> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root.join(rel_path)) {
        let entry = entry?;
        if entry.file_type().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }
    files.sort();

    let mut hasher = Sha256::new();
    for path in files {
        let repo_rel = path
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        hasher.update(repo_rel.as_bytes());
        hasher.update([0]);

        let mut file = std::fs::File::open(&path)?;
        let mut buffer = [0_u8; 8192];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        hasher.update([0]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

//! Wire-format configuration inventory, distinct from the smaller design registry.
use anyhow::{Context, Result, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use syn::{Attribute, Fields, GenericArgument, Item, ItemStruct, LitStr, PathArguments, Type};

const INPUTS: &[&str] = &[
    "crates/axon-core/src/config/parse/toml_config/raw.rs",
    "crates/axon-core/src/config/parse/toml_config.rs",
    "crates/axon-core/src/config/parse/toml_config/convert.rs",
    "xtask/src/runtime_config_docs.rs",
];
const JSON_PATH: &str = "docs/reference/config/runtime-keys.json";
const MD_PATH: &str = "docs/reference/config/runtime-keys.md";

#[derive(Serialize)]
struct Key {
    key: String,
    rust_type: String,
    defined_in: String,
}

#[derive(Default)]
struct SerdeNames {
    rename: Option<String>,
    rename_all: Option<String>,
}

fn names(attrs: &[Attribute]) -> Result<SerdeNames> {
    let mut result = SerdeNames::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("serde")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                result.rename = Some(meta.value()?.parse::<LitStr>()?.value());
            } else if meta.path.is_ident("rename_all") {
                result.rename_all = Some(meta.value()?.parse::<LitStr>()?.value());
            } else if meta.path.is_ident("deny_unknown_fields") || meta.path.is_ident("default") {
                if meta.input.peek(syn::Token![=]) {
                    let _: syn::Expr = meta.value()?.parse()?;
                }
            } else {
                return Err(meta.error("unsupported serde naming rule; extend the config-doc generator before publishing this shape"));
            }
            Ok(())
        })?;
    }
    Ok(result)
}

fn field_name(raw: &str, own: &SerdeNames, parent: &SerdeNames) -> Result<String> {
    if let Some(name) = &own.rename {
        return Ok(name.clone());
    }
    match parent.rename_all.as_deref() {
        None | Some("snake_case") => Ok(raw.to_string()),
        Some("kebab-case") => Ok(raw.replace('_', "-")),
        Some(other) => {
            bail!("unsupported config field rename rule {other}; add generator coverage")
        }
    }
}

fn type_name(ty: &Type) -> Result<String> {
    let Type::Path(path) = ty else {
        bail!("unsupported config field type; update the wire-format inventory generator");
    };
    let segment = path
        .path
        .segments
        .last()
        .context("empty config type path")?;
    let mut name = segment.ident.to_string();
    if let PathArguments::AngleBracketed(args) = &segment.arguments {
        let args = args
            .args
            .iter()
            .map(|a| match a {
                GenericArgument::Type(t) => type_name(t),
                _ => bail!("unsupported generic config type argument"),
            })
            .collect::<Result<Vec<_>>>()?;
        name.push_str(&format!("<{}>", args.join(", ")));
    }
    Ok(name)
}

fn collect(sources: &[(String, String)]) -> Result<Vec<Key>> {
    let mut structs = BTreeMap::new();
    let mut enums = BTreeSet::new();
    for (path, text) in sources {
        let file = syn::parse_file(text).with_context(|| format!("parse config input {path}"))?;
        for item in file.items {
            match item {
                Item::Struct(item) => {
                    let id = item.ident.to_string();
                    if structs.insert(id.clone(), (path.clone(), item)).is_some() {
                        bail!(
                            "duplicate config type {id}; disambiguate its module before generating docs"
                        );
                    }
                }
                Item::Enum(item) => {
                    enums.insert(item.ident.to_string());
                }
                _ => {}
            }
        }
    }
    let mut rows = Vec::new();
    visit(
        "RawTomlConfig",
        "",
        &structs,
        &enums,
        &mut BTreeSet::new(),
        &mut rows,
    )?;
    rows.sort_by(|a, b| a.key.cmp(&b.key));
    if rows.is_empty() {
        bail!("runtime configuration inventory unexpectedly empty");
    }
    if rows.windows(2).any(|pair| pair[0].key == pair[1].key) {
        bail!("duplicate serialized configuration key; fix its serde names");
    }
    Ok(rows)
}

fn visit(
    name: &str,
    prefix: &str,
    structs: &BTreeMap<String, (String, ItemStruct)>,
    enums: &BTreeSet<String>,
    stack: &mut BTreeSet<String>,
    rows: &mut Vec<Key>,
) -> Result<()> {
    if !stack.insert(name.to_string()) {
        bail!("recursive config section {name}");
    }
    let (source, item) = structs
        .get(name)
        .with_context(|| format!("missing config section type {name}"))?;
    let parent = names(&item.attrs)?;
    let Fields::Named(fields) = &item.fields else {
        bail!("config section {name} has no named fields");
    };
    for field in &fields.named {
        let raw = field
            .ident
            .as_ref()
            .context("unnamed config field")?
            .to_string();
        let segment = field_name(&raw, &names(&field.attrs)?, &parent)?;
        let key = if prefix.is_empty() {
            segment
        } else {
            format!("{prefix}.{segment}")
        };
        let display = type_name(&field.ty)?;
        let inner = display
            .strip_prefix("Option<")
            .and_then(|s| s.strip_suffix('>'))
            .unwrap_or(&display);
        if structs.contains_key(inner) {
            visit(inner, &key, structs, enums, stack, rows)?;
        } else {
            if (inner.starts_with("Raw") || inner.starts_with("Toml")) && !enums.contains(inner) {
                bail!("unresolved configuration type {inner} at {key}; include its source file");
            }
            rows.push(Key {
                key,
                rust_type: display,
                defined_in: source.clone(),
            });
        }
    }
    stack.remove(name);
    Ok(())
}

fn cell(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "&#124;")
        .replace('`', "&#96;")
        .replace('\n', "<br>")
}

pub(super) fn run(root: &Path, check: bool) -> Result<()> {
    let sources = INPUTS[..2]
        .iter()
        .map(|p| {
            Ok((
                p.to_string(),
                std::fs::read_to_string(root.join(p))
                    .with_context(|| format!("read runtime config source {p}"))?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let rows = collect(&sources)?;
    let provenance = INPUTS
        .iter()
        .map(|p| {
            let bytes = std::fs::read(root.join(p))
                .with_context(|| format!("hash runtime config source {p}"))?;
            Ok(serde_json::json!({"path":p,"sha256":format!("{:x}", Sha256::digest(bytes))}))
        })
        .collect::<Result<Vec<_>>>()?;
    let json = serde_json::json!({
        "generated_by":"cargo xtask generated-contracts refresh",
        "contract":"serialized configuration fields, not defaults or enforcement guarantees",
        "source_inputs":provenance,"keys":rows,
    });
    let mut markdown = format!(
        "# Runtime TOML Field Inventory\n\n<!-- Generated by cargo xtask generated-contracts refresh. Do not edit. -->\n\n{} serialized leaf fields, derived from Rust syntax and serde field names.\nThis is the actual wire-format shape, not the smaller normalized design registry.\n\n**Parsing is not support or enforcement.** Some fields are accepted but inert;\n`RawTomlConfig::validate_supported` rejects configured memory/graph sections and\nunsupported embedding tier limits. Check the source conversion and runtime consumer\nbefore relying on a limit, default, provider credential, or refresh behavior.\n\nUse [configuration](../../guides/configuration.md) for precedence, examples,\nand migration. [Source provenance](runtime-keys.json) records the exact inputs.\nNo secret values or guessed defaults are emitted.\n\n| Literal TOML key | Rust wire type | Defining source |\n|---|---|---|\n",
        rows.len()
    );
    for row in &rows {
        markdown.push_str(&format!(
            "| <code>{}</code> | <code>{}</code> | [{}](../../../{}) |\n",
            cell(&row.key),
            cell(&row.rust_type),
            cell(&row.defined_in),
            row.defined_in
        ));
    }
    let outputs = [
        (JSON_PATH, serde_json::to_string_pretty(&json)? + "\n"),
        (MD_PATH, markdown),
    ];
    if check {
        for (path, expected) in &outputs {
            let actual = std::fs::read_to_string(root.join(path)).with_context(|| {
                format!("missing {path}; run cargo xtask generated-contracts refresh")
            })?;
            if actual != *expected {
                bail!("{path} is stale; run cargo xtask generated-contracts refresh");
            }
        }
    } else {
        for (path, content) in &outputs {
            std::fs::write(root.join(path), content).with_context(|| format!("write {path}"))?;
        }
    }
    println!(
        "runtime config docs: {} serialized fields {}",
        rows.len(),
        if check { "verified" } else { "generated" }
    );
    Ok(())
}

#[cfg(test)]
#[path = "runtime_config_docs_tests.rs"]
mod tests;

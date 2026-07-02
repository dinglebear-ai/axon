//! Local filesystem selection rules for the target local adapter.

use std::path::Path;

use axon_api::source::*;
use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::adapter::Result;

const ALLOWED_OPTIONS: &[&str] = &[
    "include_globs",
    "exclude_globs",
    "respect_gitignore",
    "follow_symlinks",
    "max_file_bytes",
    "binary_policy",
    "watch_policy",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinaryPolicy {
    Skip,
    Metadata,
    Include,
}

#[derive(Debug, Clone)]
pub(crate) struct LocalOptions {
    pub(crate) follow_symlinks: bool,
    pub(crate) max_file_bytes: Option<u64>,
    pub(crate) respect_gitignore: bool,
    pub(crate) binary_policy: BinaryPolicy,
    include_set: Option<GlobSet>,
    sensitive_include_set: Option<GlobSet>,
    exclude_set: GlobSet,
}

impl LocalOptions {
    pub(crate) fn should_include_file(
        &self,
        scope: SourceScope,
        relative_key: &str,
        path: &Path,
    ) -> bool {
        let explicitly_sensitive_included = self
            .sensitive_include_set
            .as_ref()
            .is_some_and(|include_set| include_set.is_match(relative_key));
        if self.exclude_set.is_match(relative_key) {
            return false;
        }
        if let Some(include_set) = &self.include_set
            && !include_set.is_match(relative_key)
        {
            return false;
        }
        if scope != SourceScope::File
            && is_sensitive_local_path(relative_key)
            && !explicitly_sensitive_included
        {
            return false;
        }
        if self.binary_policy == BinaryPolicy::Skip && is_binary_path(path) {
            return false;
        }
        if scope == SourceScope::Repo {
            return is_code_search_file(path);
        }
        !is_generated_filename(file_name(path))
    }

    pub(crate) fn fetches_body(&self, path: &Path) -> bool {
        self.binary_policy != BinaryPolicy::Metadata || !is_binary_path(path)
    }

    pub(crate) fn includes_binary_body(&self, path: &Path) -> bool {
        self.binary_policy == BinaryPolicy::Include && is_binary_path(path)
    }

    pub(crate) fn should_prune_default_dirs(&self) -> bool {
        self.include_set.is_none()
    }
}

pub(crate) fn validate_options(options: &AdapterOptions) -> Result<LocalOptions> {
    for key in options.values.keys() {
        if !ALLOWED_OPTIONS.contains(&key.as_str()) {
            return Err(ApiError::new(
                "adapter.local.option.unsupported",
                axon_error::ErrorStage::Routing,
                "local adapter option is not supported",
            )
            .with_context("option", key.clone()));
        }
    }
    require_string_array(options, "include_globs")?;
    require_string_array(options, "exclude_globs")?;
    require_bool(options, "respect_gitignore")?;
    let follow_symlinks = optional_bool(options, "follow_symlinks")?.unwrap_or(false);
    let max_file_bytes = optional_u64(options, "max_file_bytes")?;
    let binary_policy = optional_binary_policy(options)?.unwrap_or(BinaryPolicy::Skip);
    require_enum(options, "watch_policy", &["manual", "auto", "disabled"])?;
    let include_globs = string_array(options, "include_globs");
    let exclude_globs = string_array(options, "exclude_globs");
    let include_set = (!include_globs.is_empty())
        .then(|| glob_set(&include_globs))
        .transpose()?;
    let sensitive_include_globs = include_globs
        .iter()
        .filter(|pattern| is_sensitive_include_pattern(pattern))
        .cloned()
        .collect::<Vec<_>>();
    let sensitive_include_set = (!sensitive_include_globs.is_empty())
        .then(|| glob_set(&sensitive_include_globs))
        .transpose()?;
    let exclude_set = glob_set(&exclude_globs)?;
    Ok(LocalOptions {
        follow_symlinks,
        max_file_bytes,
        respect_gitignore: optional_bool(options, "respect_gitignore")?.unwrap_or(true),
        binary_policy,
        include_set,
        sensitive_include_set,
        exclude_set,
    })
}

fn glob_set(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|err| option_invalid(pattern, &err.to_string()))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|err| option_invalid("glob", &err.to_string()))
}

fn string_array(options: &AdapterOptions, key: &str) -> Vec<String> {
    options
        .values
        .get(key)
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(ToString::to_string))
        .collect()
}

fn require_string_array(options: &AdapterOptions, key: &str) -> Result<()> {
    let Some(value) = options.values.get(key) else {
        return Ok(());
    };
    let valid = value
        .as_array()
        .is_some_and(|values| values.iter().all(|value| value.is_string()));
    valid
        .then_some(())
        .ok_or_else(|| option_invalid(key, "expected an array of strings"))
}

fn require_bool(options: &AdapterOptions, key: &str) -> Result<()> {
    optional_bool(options, key).map(|_| ())
}

fn optional_bool(options: &AdapterOptions, key: &str) -> Result<Option<bool>> {
    let Some(value) = options.values.get(key) else {
        return Ok(None);
    };
    value
        .as_bool()
        .map(Some)
        .ok_or_else(|| option_invalid(key, "expected a boolean"))
}

fn optional_u64(options: &AdapterOptions, key: &str) -> Result<Option<u64>> {
    let Some(value) = options.values.get(key) else {
        return Ok(None);
    };
    value
        .as_u64()
        .map(Some)
        .ok_or_else(|| option_invalid(key, "expected an unsigned integer"))
}

fn optional_binary_policy(options: &AdapterOptions) -> Result<Option<BinaryPolicy>> {
    let Some(value) = options.values.get("binary_policy") else {
        return Ok(None);
    };
    let Some(value) = value.as_str() else {
        return Err(option_invalid("binary_policy", "expected a string"));
    };
    let policy = match value {
        "skip" => BinaryPolicy::Skip,
        "metadata" => BinaryPolicy::Metadata,
        "include" => BinaryPolicy::Include,
        _ => return Err(option_invalid("binary_policy", "unsupported value")),
    };
    Ok(Some(policy))
}

fn require_enum(options: &AdapterOptions, key: &str, allowed: &[&str]) -> Result<()> {
    let Some(value) = options.values.get(key) else {
        return Ok(());
    };
    let Some(value) = value.as_str() else {
        return Err(option_invalid(key, "expected a string"));
    };
    allowed
        .contains(&value)
        .then_some(())
        .ok_or_else(|| option_invalid(key, "unsupported value"))
}

fn option_invalid(key: &str, message: &str) -> ApiError {
    ApiError::new(
        "adapter.local.option.invalid",
        axon_error::ErrorStage::Routing,
        message,
    )
    .with_context("option", key.to_string())
}

fn is_code_search_file(path: &Path) -> bool {
    if is_lockfile(file_name(path)) || is_generated_filename(file_name(path)) {
        return false;
    }
    let ext = extension(path);
    is_doc_ext(ext) || is_source_ext(ext) || is_config_ext(ext) || is_known_config(file_name(path))
}

pub(crate) fn is_pruned_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".hg"
            | ".svn"
            | ".cache"
            | ".next"
            | ".turbo"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | "coverage"
            | "vendor"
            | "__pycache__"
            | ".venv"
            | "venv"
    )
}

pub(crate) fn is_binary_path(path: &Path) -> bool {
    matches!(
        extension(path),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "ico"
            | "pdf"
            | "zip"
            | "gz"
            | "tar"
            | "7z"
            | "exe"
            | "dll"
            | "so"
            | "dylib"
            | "class"
            | "wasm"
            | "woff"
            | "woff2"
            | "ttf"
            | "otf"
    )
}

fn is_doc_ext(ext: &str) -> bool {
    matches!(ext, "md" | "mdx" | "rst" | "txt" | "adoc")
}

fn is_source_ext(ext: &str) -> bool {
    matches!(
        ext,
        "rs" | "go"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "py"
            | "java"
            | "kt"
            | "swift"
            | "c"
            | "cc"
            | "cpp"
            | "h"
            | "hpp"
            | "cs"
            | "rb"
            | "php"
            | "sh"
            | "zsh"
            | "fish"
            | "sql"
    )
}

fn is_config_ext(ext: &str) -> bool {
    matches!(ext, "json" | "yaml" | "yml" | "toml" | "xml")
}

fn is_known_config(name: &str) -> bool {
    matches!(
        name,
        "Dockerfile" | ".env.example" | "Makefile" | "Justfile"
    )
}

fn is_sensitive_local_path(relative_key: &str) -> bool {
    relative_key
        .split('/')
        .filter(|component| !component.is_empty())
        .any(is_sensitive_local_name)
}

fn is_sensitive_include_pattern(pattern: &str) -> bool {
    pattern
        .split('/')
        .filter(|component| !component.is_empty() && *component != "**" && *component != "*")
        .any(|component| {
            let concrete = component.trim_matches('*');
            !concrete.is_empty() && is_sensitive_local_name(concrete)
        })
}

fn is_sensitive_local_name(name: &str) -> bool {
    if name == ".env.example" {
        return false;
    }
    let lower = name.to_ascii_lowercase();
    lower.starts_with('.')
        || lower == "id_rsa"
        || lower == "id_ed25519"
        || lower == "known_hosts"
        || lower == "authorized_keys"
        || lower.starts_with(".env")
        || lower.ends_with(".pem")
        || lower.ends_with(".key")
        || lower.ends_with(".p12")
        || lower.ends_with(".pfx")
        || lower.contains("secret")
        || lower.contains("credential")
        || lower.contains("password")
        || lower.contains("passwd")
        || lower.contains("token")
        || lower.contains("apikey")
        || lower.contains("api-key")
        || lower.contains("api_key")
}

fn is_lockfile(name: &str) -> bool {
    matches!(
        name,
        "Cargo.lock" | "package-lock.json" | "pnpm-lock.yaml" | "yarn.lock" | "poetry.lock"
    )
}

fn is_generated_filename(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".min.js")
        || lower.ends_with(".min.css")
        || lower.ends_with(".generated.rs")
        || lower.ends_with(".pb.rs")
        || lower.ends_with(".lock")
}

fn extension(path: &Path) -> &str {
    path.extension().and_then(|ext| ext.to_str()).unwrap_or("")
}

fn file_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
}

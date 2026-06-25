use super::{is_indexable_doc_path, is_indexable_source_path, parse_github_repo};
use axon_vector::ops::input::select::{is_minified_asset_filename, is_ts_declaration_file};

// --- is_indexable_source_path ---

#[test]
fn source_path_accepts_rust_files() {
    assert!(is_indexable_source_path("src/main.rs"));
    assert!(is_indexable_source_path("lib/foo.rs"));
}

#[test]
fn source_path_accepts_python_files() {
    assert!(is_indexable_source_path("src/app.py"));
}

#[test]
fn source_path_accepts_typescript_and_js() {
    assert!(is_indexable_source_path("src/index.ts"));
    assert!(is_indexable_source_path("utils/helper.js"));
}

#[test]
fn source_path_accepts_go_files() {
    assert!(is_indexable_source_path("main.go"));
}

#[test]
fn source_path_rejects_lock_files() {
    assert!(!is_indexable_source_path("Cargo.lock"));
    assert!(!is_indexable_source_path("package-lock.json"));
    assert!(!is_indexable_source_path("pnpm-lock.yaml"));
    assert!(!is_indexable_source_path("bun.lockb"));
    assert!(!is_indexable_source_path("uv.lock"));
    assert!(!is_indexable_source_path("yarn.lock"));
    assert!(!is_indexable_source_path("Gemfile.lock"));
}

#[test]
fn source_path_rejects_generated_bulk_assets() {
    for path in [
        "apps/web/openapi/axon.json",
        "apps/web/openapi/axon.yaml",
        "openapi.json",
        "swagger.yml",
        "apps/web/lib/generated/axon-api.ts",
        "packages/sdk/generated/client.js",
        "schemas/generated/actions.json",
    ] {
        assert!(
            !is_indexable_source_path(path),
            "expected generated bulk asset to be skipped: {path}"
        );
    }
}

#[test]
fn source_path_keeps_useful_small_config_files() {
    for path in [
        "package.json",
        "config.example.toml",
        "docker-compose.yaml",
        ".github/workflows/ci.yml",
        "src/config/settings.json",
        "src/api/client.ts",
    ] {
        assert!(
            is_indexable_source_path(path),
            "expected useful config/source file to remain indexable: {path}"
        );
    }
}

#[test]
fn source_path_rejects_binary_and_image_files() {
    assert!(!is_indexable_source_path("assets/logo.png"));
    assert!(!is_indexable_source_path("icon.svg"));
    assert!(!is_indexable_source_path("font.woff2"));
}

#[test]
fn source_path_rejects_build_artifacts() {
    assert!(!is_indexable_source_path("target/release/axon"));
    assert!(!is_indexable_source_path("dist/bundle.js.map"));
    assert!(!is_indexable_source_path("node_modules/lodash/index.js"));
}

#[test]
fn source_path_rejects_language_artifact_and_cache_dirs() {
    for path in [
        ".ruff_cache/cache.py",
        ".pyre/cache.py",
        ".pytype/cache.py",
        "htmlcov/index.py",
        ".turbo/cache.ts",
        ".vitest/results.ts",
        "playwright-report/report.ts",
        "coverage/report.go",
        ".serverless/template.yml",
        ".cache/script.sh",
        "package.egg-info/meta.py",
    ] {
        assert!(
            !is_indexable_source_path(path),
            "expected artifact/cache path to be skipped: {path}"
        );
    }
}

// --- is_indexable_doc_path ---

#[test]
fn doc_path_accepts_markdown() {
    assert!(is_indexable_doc_path("README.md"));
    assert!(is_indexable_doc_path("docs/guide.md"));
    assert!(is_indexable_doc_path("CONTRIBUTING.md"));
}

#[test]
fn doc_path_rejects_generated_action_catalogs() {
    assert!(!is_indexable_doc_path("docs/reference/actions/README.md"));
    assert!(!is_indexable_doc_path("docs/reference/actions/extract.md"));
    assert!(is_indexable_doc_path("docs/reference/mcp/overview.md"));
}

#[test]
fn doc_path_rejects_source_code() {
    assert!(!is_indexable_doc_path("src/main.rs"));
}

#[test]
fn doc_path_rejects_lock_files() {
    assert!(!is_indexable_doc_path("Cargo.lock"));
}

// --- parse_github_repo ---

#[test]
fn parse_repo_from_owner_slash_repo() {
    let result = parse_github_repo("rust-lang/rust");
    assert_eq!(result, Some(("rust-lang".to_string(), "rust".to_string())));
}

#[test]
fn parse_repo_from_github_url() {
    let result = parse_github_repo("https://github.com/rust-lang/rust");
    assert_eq!(result, Some(("rust-lang".to_string(), "rust".to_string())));
}

#[test]
fn parse_repo_from_github_url_with_trailing_slash() {
    let result = parse_github_repo("https://github.com/rust-lang/rust/");
    assert_eq!(result, Some(("rust-lang".to_string(), "rust".to_string())));
}

#[test]
fn parse_repo_rejects_invalid_input() {
    assert_eq!(parse_github_repo("not-a-repo"), None);
    assert_eq!(parse_github_repo(""), None);
}

#[test]
fn parse_repo_rejects_single_component() {
    assert_eq!(parse_github_repo("rust-lang"), None);
}

#[test]
fn parse_repo_strips_git_suffix() {
    let result = parse_github_repo("https://github.com/rust-lang/rust.git");
    assert_eq!(result, Some(("rust-lang".to_string(), "rust".to_string())));
}

#[test]
fn parse_repo_strips_git_suffix_bare() {
    let result = parse_github_repo("rust-lang/rust.git");
    assert_eq!(result, Some(("rust-lang".to_string(), "rust".to_string())));
}

#[test]
fn parse_repo_rejects_empty_after_git_strip() {
    // ".git" is the entire repo component — stripping it yields an empty repo
    assert_eq!(parse_github_repo("owner/.git"), None);
    assert_eq!(parse_github_repo("https://github.com/owner/.git"), None);
}

// --- expanded extensions ---

#[test]
fn source_path_accepts_c_cpp_files() {
    assert!(is_indexable_source_path("src/main.c"));
    assert!(is_indexable_source_path("src/main.cpp"));
    assert!(is_indexable_source_path("include/header.h"));
    assert!(is_indexable_source_path("include/header.hpp"));
}

#[test]
fn source_path_accepts_java_kotlin_files() {
    assert!(is_indexable_source_path("src/App.java"));
    assert!(is_indexable_source_path("src/App.kt"));
}

#[test]
fn source_path_accepts_ruby_php_shell() {
    assert!(is_indexable_source_path("lib/helper.rb"));
    assert!(is_indexable_source_path("src/index.php"));
    assert!(is_indexable_source_path("scripts/deploy.sh"));
}

#[test]
fn source_path_accepts_yaml_json_md() {
    assert!(is_indexable_source_path("config/settings.yaml"));
    assert!(is_indexable_source_path("config/settings.yml"));
    assert!(is_indexable_source_path("package.json"));
    assert!(is_indexable_source_path("README.md"));
}

// --- is_ts_declaration_file ---

#[test]
fn declaration_file_rejects_d_ts_variants() {
    assert!(is_ts_declaration_file("index.d.ts"));
    assert!(is_ts_declaration_file("types.d.mts"));
    assert!(is_ts_declaration_file("module.d.cts"));
}

#[test]
fn declaration_file_accepts_plain_ts() {
    assert!(!is_ts_declaration_file("index.ts"));
    assert!(!is_ts_declaration_file("component.tsx"));
    assert!(!is_ts_declaration_file("util.js"));
}

#[test]
fn source_path_rejects_declaration_files() {
    assert!(!is_indexable_source_path("src/types/index.d.ts"));
    assert!(!is_indexable_source_path("dist/index.d.mts"));
    assert!(!is_indexable_source_path("lib/module.d.cts"));
}

// --- is_minified_asset_filename ---

#[test]
fn minified_asset_rejects_min_js_and_bundle() {
    assert!(is_minified_asset_filename("app.min.js"));
    assert!(is_minified_asset_filename("vendor.min.mjs"));
    assert!(is_minified_asset_filename("styles.min.css"));
    assert!(is_minified_asset_filename("main.bundle.js"));
    assert!(is_minified_asset_filename("runtime.bundle.mjs"));
}

#[test]
fn minified_asset_accepts_normal_js_css() {
    assert!(!is_minified_asset_filename("index.js"));
    assert!(!is_minified_asset_filename("styles.css"));
    assert!(!is_minified_asset_filename("app.ts"));
}

#[test]
fn source_path_rejects_minified_assets() {
    assert!(!is_indexable_source_path("public/app.min.js"));
    assert!(!is_indexable_source_path("assets/styles.min.css"));
    assert!(!is_indexable_source_path("dist/vendor.bundle.js"));
}

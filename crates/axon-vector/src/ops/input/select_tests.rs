use super::*;

#[test]
fn prunes_vcs_and_build_dirs() {
    for name in [
        ".git",
        "node_modules",
        "__pycache__",
        ".ruff_cache",
        ".pyre",
        ".pytype",
        "htmlcov",
        ".turbo",
        ".vitest",
        "playwright-report",
        "target",
        "dist",
        "coverage",
        ".serverless",
        ".venv",
    ] {
        assert!(is_pruned_dir(name), "expected {name} to be pruned");
    }
}

#[test]
fn detects_pruned_path_components() {
    for path in [
        "target/debug/axon",
        "apps/web/.turbo/cache.ts",
        "src\\__pycache__\\cache.py",
        "package.egg-info/meta.py",
    ] {
        assert!(has_pruned_component(path), "expected {path} to be pruned");
    }
    assert!(!has_pruned_component("src/target_helpers/mod.rs"));
}

#[test]
fn does_not_prune_normal_dirs() {
    for name in ["src", "docs", "tests", "lib", "my_target_helpers"] {
        assert!(!is_pruned_dir(name), "did not expect {name} to be pruned");
    }
}

#[test]
fn flags_binary_extensions_case_insensitively() {
    for ext in ["png", "PNG", "woff2", "pdf", "so", "sqlite3", "mp4"] {
        assert!(is_binary_ext(ext), "expected {ext} to be binary");
    }
}

#[test]
fn does_not_flag_text_extensions() {
    for ext in ["rs", "md", "txt", "log", "csv", "toml", "json", ""] {
        assert!(!is_binary_ext(ext), "did not expect {ext} to be binary");
    }
}

#[test]
fn detects_code_paths() {
    for path in [
        "src/lib.rs",
        "a/b/main.PY",
        "app.tsx",
        "script.sh",
        "pkg/handler.go",
        // Structured-data formats route through tree-sitter too (the routing
        // predicate delegates to language_for_extension, so it stays in sync).
        "package.json",
        "compose.yaml",
        "settings.YML",
        "config.toml",
    ] {
        assert!(
            should_chunk_as_code(path),
            "expected {path} to chunk as code"
        );
    }
}

#[test]
fn non_code_paths_are_not_code() {
    for path in [
        "README.md",
        "notes.txt",
        "data.csv",
        "image.png",
        "Dockerfile",
        "Makefile",
    ] {
        assert!(
            !should_chunk_as_code(path),
            "did not expect {path} to chunk as code"
        );
    }
}

#[test]
fn dotfiles_and_extensionless_are_not_code() {
    // path_extension yields "" for these, so they must route to prose, not AST.
    // Pins the behavior so a future path_extension change can't silently start
    // AST-chunking config/dotfiles.
    for path in [".gitignore", ".env", ".dockerignore", "LICENSE", "Makefile"] {
        assert!(
            !should_chunk_as_code(path),
            "did not expect {path} to chunk as code"
        );
    }
}

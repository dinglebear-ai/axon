use super::*;

#[test]
fn prunes_vcs_and_build_dirs() {
    for name in [
        ".git",
        "node_modules",
        "__pycache__",
        "target",
        "dist",
        ".venv",
    ] {
        assert!(is_pruned_dir(name), "expected {name} to be pruned");
    }
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
        "config.toml",
        "Dockerfile",
        "Makefile",
    ] {
        assert!(
            !should_chunk_as_code(path),
            "did not expect {path} to chunk as code"
        );
    }
}

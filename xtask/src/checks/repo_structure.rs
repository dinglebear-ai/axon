use std::fs;
use std::path::Path;

pub const TARGET_NEW_CRATES: &[&str] = &[
    "axon-error",
    "axon-observe",
    "axon-route",
    "axon-adapters",
    "axon-ledger",
    "axon-parse",
    "axon-graph",
    "axon-memory",
    "axon-document",
    "axon-embedding",
    "axon-vectors",
    "axon-retrieval",
    "axon-llm",
    "axon-prune",
];

pub const TRANSITIONAL_CRATES: &[&str] = &[
    "axon-crawl",
    "axon-vector",
    "axon-ingest",
    "axon-extract",
    "axon-jobs",
    "axon-source-ledger",
    "axon-code-index",
];

pub const EXISTING_STABLE_CRATES: &[&str] = &[
    "axon-api",
    "axon-authz",
    "axon-core",
    "axon-services",
    "axon-mcp",
    "axon-web",
    "axon-cli",
];

pub fn check(root: &Path) -> anyhow::Result<()> {
    check_root(root).map_err(anyhow::Error::msg)
}

pub fn check_root(root: &Path) -> Result<(), String> {
    let mut errors = Vec::new();
    let cargo_toml = read(root.join("Cargo.toml"), &mut errors);

    for krate in TARGET_NEW_CRATES {
        check_target_crate(root, krate, &cargo_toml, &mut errors);
    }

    for krate in TRANSITIONAL_CRATES
        .iter()
        .chain(EXISTING_STABLE_CRATES.iter())
    {
        check_workspace_member(root, krate, &cargo_toml, &mut errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn check_target_crate(root: &Path, krate: &str, cargo_toml: &str, errors: &mut Vec<String>) {
    if krate.matches('-').count() != 1 {
        errors.push(format!(
            "target crate has invalid double-hyphen-style name: {krate}"
        ));
    }

    check_workspace_member(root, krate, cargo_toml, errors);

    let crate_root = root.join("crates").join(krate);
    require_file(&crate_root.join("Cargo.toml"), errors);
    require_file(&crate_root.join("src/lib.rs"), errors);
    require_file(&crate_root.join("src/CLAUDE.md"), errors);
    require_claude_symlink(&crate_root.join("src/AGENTS.md"), errors);
    require_claude_symlink(&crate_root.join("src/GEMINI.md"), errors);
}

fn check_workspace_member(root: &Path, krate: &str, cargo_toml: &str, errors: &mut Vec<String>) {
    let crate_dir = format!("crates/{krate}");
    if !root.join(&crate_dir).is_dir() {
        errors.push(format!("missing target crate directory: {crate_dir}"));
    }
    if !cargo_toml.contains(&format!("\"{crate_dir}\"")) {
        errors.push(format!(
            "root Cargo.toml is missing workspace member: {crate_dir}"
        ));
    }
}

fn require_file(path: &Path, errors: &mut Vec<String>) {
    if !path.is_file() {
        errors.push(format!("missing required file: {}", display(path)));
    }
}

fn require_claude_symlink(path: &Path, errors: &mut Vec<String>) {
    match fs::read_link(path) {
        Ok(target) if target == Path::new("CLAUDE.md") => {}
        Ok(_) => errors.push(format!("{} must symlink to CLAUDE.md", display(path))),
        Err(_) => errors.push(format!("missing required symlink: {}", display(path))),
    }
}

fn read(path: impl AsRef<Path>, errors: &mut Vec<String>) -> String {
    let path = path.as_ref();
    match fs::read_to_string(path) {
        Ok(body) => body,
        Err(err) => {
            errors.push(format!("failed to read {}: {err}", display(path)));
            String::new()
        }
    }
}

fn display(path: &Path) -> String {
    path.strip_prefix(".").unwrap_or(path).display().to_string()
}

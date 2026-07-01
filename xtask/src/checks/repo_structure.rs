use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

const TARGET_RUST_VERSION: &str = "1.94.0";
const DEPENDENCY_TABLES: &[&str] = &["dependencies", "dev-dependencies", "build-dependencies"];

pub const REQUIRED_WORKSPACE_MEMBERS: &[&str] = &[
    "xtask",
    "crates/axon-api",
    "crates/axon-authz",
    "crates/axon-core",
    "crates/axon-crawl",
    "crates/axon-vector",
    "crates/axon-ingest",
    "crates/axon-extract",
    "crates/axon-jobs",
    "crates/axon-source-ledger",
    "crates/axon-code-index",
    "crates/axon-services",
    "crates/axon-mcp",
    "crates/axon-web",
    "crates/axon-cli",
];

pub struct TargetCrate {
    pub name: &'static str,
    pub modules: &'static [&'static str],
}

pub const TARGET_CRATES: &[TargetCrate] = &[
    TargetCrate {
        name: "axon-error",
        modules: &[
            "api_error",
            "code",
            "stage",
            "severity",
            "retry",
            "degradation",
            "cooling",
            "context",
            "conversion",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-observe",
        modules: &[
            "event",
            "phase",
            "heartbeat",
            "progress",
            "metric",
            "span",
            "log",
            "collector",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-route",
        modules: &[
            "resolver",
            "router",
            "canonical",
            "source_id",
            "scope",
            "authority",
            "alias",
            "capability",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-adapters",
        modules: &[
            "adapter",
            "registry",
            "capability",
            "acquisition",
            "manifest",
            "web",
            "local",
            "git",
            "registry_sources",
            "feed",
            "youtube",
            "reddit",
            "sessions",
            "cli_tool",
            "mcp_tool",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-ledger",
        modules: &[
            "store",
            "sqlite",
            "migration",
            "source",
            "item",
            "manifest",
            "diff",
            "generation",
            "document_status",
            "lease",
            "cleanup_debt",
            "transaction",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-parse",
        modules: &[
            "parser",
            "registry",
            "facts",
            "graph_candidate",
            "code",
            "manifest",
            "schema",
            "session",
            "tool",
            "env",
            "docker",
            "config",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-graph",
        modules: &[
            "store",
            "sqlite",
            "migration",
            "node",
            "edge",
            "evidence",
            "candidate",
            "authority",
            "merge",
            "query",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-memory",
        modules: &[
            "store",
            "sqlite",
            "migration",
            "record",
            "link",
            "decay",
            "review",
            "recall",
            "context",
            "graph",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-document",
        modules: &[
            "preparer",
            "chunk_router",
            "profile",
            "prepared",
            "chunk",
            "metadata",
            "code",
            "markdown",
            "transcript",
            "session",
            "schema",
            "text",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-embedding",
        modules: &[
            "provider",
            "batch",
            "capability",
            "reservation",
            "tei",
            "openai_compat",
            "fake",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-vectors",
        modules: &[
            "store",
            "qdrant",
            "collection",
            "point",
            "payload",
            "filter",
            "query",
            "health",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-retrieval",
        modules: &[
            "engine", "plan", "query", "filter", "rank", "context", "citation", "memory", "graph",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-llm",
        modules: &[
            "provider",
            "capability",
            "completion",
            "stream",
            "prompt",
            "openai_compat",
            "codex",
            "gemini",
            "fake",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-prune",
        modules: &[
            "plan",
            "executor",
            "debt",
            "generation",
            "orphan",
            "dedupe",
            "receipt",
            "safety",
            "testing",
        ],
    },
];

pub fn check(root: &Path) -> anyhow::Result<()> {
    check_root(root).map_err(anyhow::Error::msg)
}

pub fn check_root(root: &Path) -> Result<(), String> {
    let mut errors = Vec::new();
    let cargo_toml = read(root.join("Cargo.toml"), &mut errors);
    require_workspace_rust_version(&cargo_toml, &mut errors);
    let workspace_members = workspace_members(&cargo_toml, &mut errors);

    for member in REQUIRED_WORKSPACE_MEMBERS {
        require_workspace_member_path(member, &workspace_members, &mut errors);
    }

    for krate in TARGET_CRATES {
        check_target_crate(root, krate, &workspace_members, &mut errors);
    }

    for member in &workspace_members {
        if !root.join(member).is_dir() {
            errors.push(format!("workspace member path does not exist: {member}"));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn check_target_crate(
    root: &Path,
    krate: &TargetCrate,
    workspace_members: &BTreeSet<String>,
    errors: &mut Vec<String>,
) {
    require_workspace_member(krate.name, workspace_members, errors);

    let crate_root = root.join("crates").join(krate.name);
    let crate_toml = read(crate_root.join("Cargo.toml"), errors);
    if let Some(manifest) = parse_target_manifest(krate.name, &crate_toml, errors) {
        require_target_manifest_metadata(krate.name, &manifest, errors);
        require_empty_dependency_tables(krate.name, &manifest, errors);
    }

    let src_dir = crate_root.join("src");
    let lib_rs = read(src_dir.join("lib.rs"), errors);
    require_modules(krate, &src_dir, &lib_rs, errors);
    require_file(&src_dir.join("CLAUDE.md"), errors);
    require_claude_symlink(&src_dir.join("AGENTS.md"), errors);
    require_claude_symlink(&src_dir.join("GEMINI.md"), errors);
}

fn require_workspace_member(
    krate: &str,
    workspace_members: &BTreeSet<String>,
    errors: &mut Vec<String>,
) {
    let crate_dir = format!("crates/{krate}");
    require_workspace_member_path(&crate_dir, workspace_members, errors);
}

fn require_workspace_member_path(
    member: &str,
    workspace_members: &BTreeSet<String>,
    errors: &mut Vec<String>,
) {
    if !workspace_members.contains(member) {
        errors.push(format!(
            "root Cargo.toml is missing workspace member: {member}"
        ));
    }
}

fn require_workspace_rust_version(cargo_toml: &str, errors: &mut Vec<String>) {
    let parsed = match toml::from_str::<toml::Table>(cargo_toml) {
        Ok(parsed) => parsed,
        Err(_) => return,
    };

    let actual = parsed
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("rust-version"))
        .and_then(toml::Value::as_str);

    if actual != Some(TARGET_RUST_VERSION) {
        errors.push(format!(
            "root Cargo.toml must set workspace.package.rust-version = {TARGET_RUST_VERSION:?}"
        ));
    }
}

fn require_file(path: &Path, errors: &mut Vec<String>) {
    if !path.is_file() {
        errors.push(format!("missing required file: {}", display(path)));
    }
}

fn require_modules(krate: &TargetCrate, src_dir: &Path, lib_rs: &str, errors: &mut Vec<String>) {
    let expected_modules = krate.modules.iter().copied().collect::<BTreeSet<_>>();
    let declarations = lib_rs.lines().map(str::trim).collect::<BTreeSet<_>>();
    let public_modules = lib_rs
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            line.strip_prefix("pub mod ")
                .and_then(|module| module.strip_suffix(';'))
        })
        .collect::<BTreeSet<_>>();

    for module in krate.modules {
        require_file(&src_dir.join(format!("{module}.rs")), errors);

        let expected_decl = format!("pub mod {module};");
        if !declarations.contains(expected_decl.as_str()) {
            errors.push(format!(
                "{}/lib.rs is missing module declaration: {expected_decl}",
                display(src_dir)
            ));
        }
    }

    for module in public_modules.difference(&expected_modules) {
        errors.push(format!(
            "{}/lib.rs declares unexpected PR0 public module: pub mod {module};",
            display(src_dir)
        ));
    }

    match fs::read_dir(src_dir) {
        Ok(entries) => {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.file_name().and_then(|name| name.to_str()) == Some("lib.rs") {
                    continue;
                }
                if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                    continue;
                }
                let Some(module) = path.file_stem().and_then(|stem| stem.to_str()) else {
                    continue;
                };
                if !expected_modules.contains(module) {
                    errors.push(format!(
                        "{} is an unexpected PR0 module file",
                        display(&path)
                    ));
                }
            }
        }
        Err(err) => errors.push(format!("failed to read {}: {err}", display(src_dir))),
    }
}

fn require_claude_symlink(path: &Path, errors: &mut Vec<String>) {
    match fs::read_link(path) {
        Ok(target) if target == Path::new("CLAUDE.md") => {}
        Ok(_) => errors.push(format!("{} must symlink to CLAUDE.md", display(path))),
        Err(_) => errors.push(format!("missing required symlink: {}", display(path))),
    }
}

fn parse_target_manifest(
    krate: &str,
    cargo_toml: &str,
    errors: &mut Vec<String>,
) -> Option<toml::Table> {
    let parsed = match toml::from_str::<toml::Table>(cargo_toml) {
        Ok(parsed) => parsed,
        Err(err) => {
            errors.push(format!("failed to parse crates/{krate}/Cargo.toml: {err}"));
            return None;
        }
    };
    Some(parsed)
}

fn require_target_manifest_metadata(krate: &str, parsed: &toml::Table, errors: &mut Vec<String>) {
    let Some(package) = parsed.get("package").and_then(toml::Value::as_table) else {
        errors.push(format!("crates/{krate}/Cargo.toml is missing [package]"));
        return;
    };

    if package.get("name").and_then(toml::Value::as_str) != Some(krate) {
        errors.push(format!(
            "PR0 target crate {krate} must set package.name = {krate:?}"
        ));
    }

    match package.get("rust-version") {
        Some(value)
            if value
                .as_table()
                .and_then(|table| table.get("workspace"))
                .and_then(toml::Value::as_bool)
                == Some(true) => {}
        _ => errors.push(format!(
            "PR0 target crate {krate} must set rust-version.workspace = true (workspace rust-version {TARGET_RUST_VERSION})"
        )),
    }
}

fn require_empty_dependency_tables(krate: &str, parsed: &toml::Table, errors: &mut Vec<String>) {
    let mut dependency_tables = Vec::new();
    collect_non_empty_dependency_tables(
        &toml::Value::Table(parsed.clone()),
        &mut Vec::new(),
        &mut dependency_tables,
    );

    for table_name in dependency_tables {
        errors.push(format!(
            "PR0 target crate {krate} must keep {table_name} empty"
        ));
    }
}

fn collect_non_empty_dependency_tables(
    value: &toml::Value,
    path: &mut Vec<String>,
    found: &mut Vec<String>,
) {
    let Some(table) = value.as_table() else {
        return;
    };

    for (key, value) in table {
        path.push(key.clone());
        if DEPENDENCY_TABLES.contains(&key.as_str())
            && value.as_table().is_some_and(|table| !table.is_empty())
        {
            found.push(format!("[{}]", path.join(".")));
        } else {
            collect_non_empty_dependency_tables(value, path, found);
        }
        path.pop();
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

fn workspace_members(cargo_toml: &str, errors: &mut Vec<String>) -> BTreeSet<String> {
    let parsed = match toml::from_str::<toml::Table>(cargo_toml) {
        Ok(parsed) => parsed,
        Err(err) => {
            errors.push(format!("failed to parse Cargo.toml: {err}"));
            return BTreeSet::new();
        }
    };

    match parsed
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array)
    {
        Some(members) => members
            .iter()
            .filter_map(|member| match member.as_str() {
                Some(member) => Some(member.to_string()),
                None => {
                    errors.push("workspace member must be a string".to_string());
                    None
                }
            })
            .collect(),
        None => {
            errors.push("root Cargo.toml is missing [workspace].members".to_string());
            BTreeSet::new()
        }
    }
}

fn display(path: &Path) -> String {
    path.strip_prefix(".").unwrap_or(path).display().to_string()
}

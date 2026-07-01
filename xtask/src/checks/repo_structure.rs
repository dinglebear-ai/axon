use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

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
    let workspace_members = workspace_members(&cargo_toml, &mut errors);

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
    require_empty_dependency_tables(krate.name, &crate_toml, errors);

    let lib_rs = read(crate_root.join("src/lib.rs"), errors);
    require_modules(krate, &crate_root, &lib_rs, errors);
    require_file(&crate_root.join("src/CLAUDE.md"), errors);
    require_claude_symlink(&crate_root.join("src/AGENTS.md"), errors);
    require_claude_symlink(&crate_root.join("src/GEMINI.md"), errors);
}

fn require_workspace_member(
    krate: &str,
    workspace_members: &BTreeSet<String>,
    errors: &mut Vec<String>,
) {
    let crate_dir = format!("crates/{krate}");
    if !workspace_members.contains(&crate_dir) {
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

fn require_modules(krate: &TargetCrate, crate_root: &Path, lib_rs: &str, errors: &mut Vec<String>) {
    for module in krate.modules {
        require_file(&crate_root.join("src").join(format!("{module}.rs")), errors);

        let expected_decl = format!("pub mod {module};");
        if !lib_rs.lines().any(|line| line.trim() == expected_decl) {
            errors.push(format!(
                "{}/src/lib.rs is missing module declaration: {expected_decl}",
                display(crate_root)
            ));
        }
    }
}

fn require_claude_symlink(path: &Path, errors: &mut Vec<String>) {
    match fs::read_link(path) {
        Ok(target) if target == Path::new("CLAUDE.md") => {}
        Ok(_) => errors.push(format!("{} must symlink to CLAUDE.md", display(path))),
        Err(_) => errors.push(format!("missing required symlink: {}", display(path))),
    }
}

fn require_empty_dependency_tables(krate: &str, cargo_toml: &str, errors: &mut Vec<String>) {
    let parsed = match toml::from_str::<toml::Table>(cargo_toml) {
        Ok(parsed) => parsed,
        Err(err) => {
            errors.push(format!("failed to parse crates/{krate}/Cargo.toml: {err}"));
            return;
        }
    };

    for table_name in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(table) = parsed.get(table_name).and_then(toml::Value::as_table)
            && !table.is_empty()
        {
            errors.push(format!(
                "PR0 target crate {krate} must keep [{table_name}] empty"
            ));
        }
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

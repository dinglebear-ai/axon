use anyhow::{Context, Result};
use clap::Parser;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Parser)]
pub struct PrePushArgs {
    /// Print the selected plan without running commands.
    #[arg(long)]
    pub dry_run: bool,
    /// Read changed paths from a file instead of diffing git.
    #[arg(long)]
    pub changed_files: Option<PathBuf>,
}

#[derive(Debug, Default)]
struct Categories {
    docs: bool,
    workflow: bool,
    rust: bool,
    web: bool,
    android: bool,
    palette: bool,
    chrome: bool,
    docker: bool,
    compose: bool,
    mcp: bool,
    security: bool,
    release: bool,
    version_files: bool,
    openapi: bool,
    codeql_actions: bool,
    codeql_python: bool,
    codeql_rust: bool,
}

impl Categories {
    fn all() -> Self {
        Self {
            docs: true,
            workflow: true,
            rust: true,
            web: true,
            android: true,
            palette: true,
            chrome: true,
            docker: true,
            compose: true,
            mcp: true,
            security: true,
            release: true,
            version_files: true,
            openapi: true,
            codeql_actions: true,
            codeql_python: true,
            codeql_rust: true,
        }
    }

    fn names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        for (name, enabled) in [
            ("docs", self.docs),
            ("workflow", self.workflow),
            ("rust", self.rust),
            ("web", self.web),
            ("android", self.android),
            ("palette", self.palette),
            ("chrome", self.chrome),
            ("docker", self.docker),
            ("compose", self.compose),
            ("mcp", self.mcp),
            ("security", self.security),
            ("release", self.release),
            ("version_files", self.version_files),
            ("openapi", self.openapi),
            ("codeql_actions", self.codeql_actions),
            ("codeql_python", self.codeql_python),
            ("codeql_rust", self.codeql_rust),
        ] {
            if enabled {
                names.push(name);
            }
        }
        names
    }
}

#[derive(Debug)]
struct PlanStep {
    name: &'static str,
    command: &'static str,
}

pub fn run(root: &Path, args: PrePushArgs) -> Result<()> {
    let full = truthy(std::env::var("AXON_FULL_PRE_PUSH").ok().as_deref());
    let paths = if let Some(path) = args.changed_files {
        read_changed_files(&path)?
    } else {
        match resolve_base(root).and_then(|base| changed_files(root, &base)) {
            Ok(paths) => paths,
            Err(error) => {
                if !full {
                    eprintln!(
                        "pre-push: could not determine changed files ({error:#}); running minimal \
                         checks only (CI is authoritative). Set AXON_FULL_PRE_PUSH=1 for full local validation."
                    );
                }
                Vec::new()
            }
        }
    };

    let categories = classify(&paths, full);
    let plan = command_plan(&paths, &categories, full);

    write_classifier_output(&paths, &categories);
    println!("Pre-push plan:");
    if plan.is_empty() {
        println!("  <none>");
    } else {
        for step in &plan {
            println!("  {}: {}", step.name, step.command);
        }
    }

    if args.dry_run {
        return Ok(());
    }

    for step in plan {
        run_command(root, &step)?;
    }
    Ok(())
}

fn truthy(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn resolve_base(root: &Path) -> Result<String> {
    if let Ok(override_base) = std::env::var("AXON_PRE_PUSH_BASE")
        && !override_base.trim().is_empty()
    {
        return Ok(override_base);
    }

    for candidate in ["@{upstream}", "origin/main"] {
        if !git_ref_exists(root, candidate) {
            continue;
        }
        if let Ok(base) = git_output(root, &["merge-base", candidate, "HEAD"]) {
            return Ok(base);
        }
    }

    git_output(root, &["rev-parse", "HEAD^"]).context("failed to resolve HEAD^ fallback")
}

fn git_ref_exists(root: &Path, reference: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--verify", "--quiet", reference])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn changed_files(root: &Path, base: &str) -> Result<Vec<String>> {
    let raw = git_output(root, &["diff", "--name-only", base, "HEAD"])?;
    Ok(raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {args:?}"))?;
    if !output.status.success() {
        anyhow::bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn read_changed_files(path: &Path) -> Result<Vec<String>> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read changed-files input {}", path.display()))?;
    Ok(raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn classify(paths: &[String], full: bool) -> Categories {
    if full {
        return Categories::all();
    }

    let workflow_paths = paths
        .iter()
        .filter(|path| is_workflow_router_path(path))
        .collect::<Vec<_>>();
    let runtime_paths = paths.iter().map(String::as_str).collect::<Vec<_>>();
    let mut categories = classify_runtime(&runtime_paths);
    if !workflow_paths.is_empty() {
        categories.workflow = true;
        categories.codeql_actions = true;
        if workflow_paths.iter().any(|path| path.ends_with(".py")) {
            categories.codeql_python = true;
        }
    }
    categories
}

fn classify_runtime(paths: &[&str]) -> Categories {
    let openapi = any_path(paths, &["apps/web/openapi/"]);
    let rust = any_path(
        paths,
        &[
            "src/",
            "crates/",
            "xtask/",
            "benches/",
            "tests/",
            "migrations/",
            "vendor/",
            ".cargo/",
            ".config/",
        ],
    ) || any_file(
        paths,
        &[
            "Cargo.toml",
            "Cargo.lock",
            "build.rs",
            "rust-toolchain.toml",
            "Justfile",
        ],
    ) || paths
        .iter()
        .any(|path| contains_path(rust_ci_helper_scripts(), path));
    let web = openapi || any_path(paths, &["apps/web/", "assets/"]);
    let android = openapi || any_path(paths, &["apps/android/"]);
    let palette = openapi || any_path(paths, &["apps/palette-tauri/"]);
    let chrome = any_path(paths, &["apps/chrome-extension/", "assets/"]);
    let docs = any_path(paths, &["docs/"])
        || any_file(paths, &["README.md", "CHANGELOG.md"])
        || paths
            .iter()
            .any(|path| contains_path(doc_ci_helper_scripts(), path));
    let version_files = any_file(paths, &["README.md", "CHANGELOG.md"]);
    let mcp = any_path(
        paths,
        &["src/mcp/", "crates/axon-mcp/src/", "docs/reference/mcp/"],
    ) || paths
        .iter()
        .any(|path| contains_path(mcp_ci_helper_scripts(), path))
        || any_file(paths, &["tests/workflow_shapes.rs"]);
    let release = rust || web || any_path(paths, &["release/"]);
    let compose = any_path(paths, &["config/", "scripts/"])
        || any_file(
            paths,
            &[
                ".dockerignore",
                ".env.example",
                "docker-compose.yaml",
                "docker-compose.prod.yaml",
                "docker-compose.llama.yaml",
            ],
        );
    let docker = rust || web || compose || any_file(paths, &[".dockerignore", "config/Dockerfile"]);
    let security = rust
        || any_file(paths, &["Cargo.lock", "deny.toml"])
        || any_path(paths, &[".cargo/", "vendor/"]);

    Categories {
        docs,
        workflow: false,
        rust,
        web,
        android,
        palette,
        chrome,
        docker,
        compose,
        mcp,
        security,
        release,
        version_files,
        openapi,
        codeql_actions: false,
        codeql_python: paths.iter().any(|path| path.ends_with(".py")),
        codeql_rust: rust || palette,
    }
}

fn command_plan(paths: &[String], categories: &Categories, full: bool) -> Vec<PlanStep> {
    let workflow_changed = paths.iter().any(|path| {
        starts(path, &[".github/workflows/"])
            || contains_path(workflow_router_paths(), path)
            || path == "xtask/src/pre_push.rs"
    });
    let env_boundary_changed = any_file_str(
        paths,
        &[
            "scripts/check-env-config-boundary.py",
            "tests/env_config_boundary.rs",
        ],
    );
    let rust_api_changed = paths.iter().any(|path| {
        starts(
            path,
            &[
                "src/web/",
                "src/services/",
                "src/mcp/",
                "src/cli/commands/rest/",
                "crates/axon-web/src/",
                "crates/axon-services/src/",
                "crates/axon-mcp/src/",
                "crates/axon-cli/src/commands/rest/",
            ],
        )
    });
    let android_app_changed = paths.iter().any(|path| starts(path, &["apps/android/"]));

    let mut plan = Vec::new();
    if full
        || categories.android
        || categories.palette
        || categories.chrome
        || categories.version_files
    {
        plan.push(PlanStep {
            name: "version-sync",
            command: "cargo xtask check-version-sync",
        });
    }
    if workflow_changed {
        plan.extend([
            PlanStep {
                name: "workflow-lint",
                command: "actionlint .github/workflows/ci.yml .github/workflows/codeql.yml .github/workflows/compose-smoke.yml .github/workflows/docker-image.yml",
            },
            PlanStep {
                name: "ci-path-tests",
                command: "cargo test --locked --test ci_changed_paths",
            },
            PlanStep {
                name: "workflow-shape-tests",
                command: "cargo test --locked --test workflow_shapes",
            },
        ]);
    }
    if env_boundary_changed {
        plan.push(PlanStep {
            name: "env-boundary-test",
            command: "cargo test --locked --features test-helpers --test env_config_boundary env_config_boundary_matrix_is_current -- --nocapture",
        });
    }
    if categories.web {
        plan.push(PlanStep {
            name: "web-assets",
            command: "if [ ! -d apps/web/node_modules ]; then npm ci --prefix apps/web; fi && npm --prefix apps/web run build",
        });
    }
    if categories.rust {
        plan.push(PlanStep {
            name: "web-assets-placeholder",
            command: "mkdir -p apps/web/out",
        });
        plan.push(PlanStep {
            name: "repo-structure",
            command: "cargo xtask check-repo-structure",
        });
        plan.push(PlanStep {
            name: "clippy",
            command: "AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo clippy --workspace --all-targets --locked -- -D warnings",
        });
    }
    if full {
        plan.push(PlanStep {
            name: "full-nextest",
            command: "AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo nextest run --workspace --locked --lib -E 'not test(/worker_e2e/)'",
        });
    }
    if full || categories.openapi || rust_api_changed || categories.android || categories.palette {
        plan.push(PlanStep {
            name: "openapi-drift",
            command: "cargo xtask check-openapi-drift",
        });
    }
    if android_app_changed {
        plan.push(PlanStep {
            name: "android",
            command: "if [ -z \"${AXON_AURORA_ANDROID_PATH:-}\" ]; then for candidate in ../aurora-design-system/android ../../../aurora-design-system/android /home/jmagar/workspace/aurora-design-system/android; do if [ -d \"$candidate\" ]; then export AXON_AURORA_ANDROID_PATH=\"$candidate\"; break; fi; done; fi; if [ ! -d \"${AXON_AURORA_ANDROID_PATH:-}\" ]; then echo 'Set AXON_AURORA_ANDROID_PATH to an Aurora Android checkout before running Android validation.' >&2; exit 1; fi; apps/android/gradlew -p apps/android :app:testDebugUnitTest :app:lintDebug --no-daemon",
        });
    }
    dedupe_plan(plan)
}

fn dedupe_plan(plan: Vec<PlanStep>) -> Vec<PlanStep> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for step in plan {
        if seen.insert(step.name) {
            out.push(step);
        }
    }
    out
}

fn run_command(root: &Path, step: &PlanStep) -> Result<()> {
    println!("\n==> {}\n{}", step.name, step.command);
    let mut command = Command::new("bash");
    command
        .arg("-lc")
        .arg(step.command)
        .current_dir(root)
        .env("AXON_ALLOW_FALLBACK_WEB_ASSETS", "1");
    for (key, _) in std::env::vars() {
        if key.starts_with("CARGO_PROFILE_") {
            command.env_remove(key);
        }
    }
    let status = command
        .status()
        .with_context(|| format!("failed to run {}", step.name))?;
    if !status.success() {
        anyhow::bail!("{} failed with {status}", step.name);
    }
    Ok(())
}

fn write_classifier_output(paths: &[String], categories: &Categories) {
    println!("Changed files:");
    if paths.is_empty() {
        println!("  <none relative to selected base>");
    } else {
        for path in paths {
            println!("  {path}");
        }
    }
    let names = categories.names();
    if names.is_empty() {
        println!("Enabled categories: <none>");
    } else {
        println!("Enabled categories: {}", names.join(", "));
    }
}

fn starts(path: &str, prefixes: &[&str]) -> bool {
    prefixes
        .iter()
        .any(|prefix| path == prefix.trim_end_matches('/') || path.starts_with(prefix))
}

fn any_path(paths: &[&str], prefixes: &[&str]) -> bool {
    paths.iter().any(|path| starts(path, prefixes))
}

fn any_file(paths: &[&str], names: &[&str]) -> bool {
    paths.iter().any(|path| names.contains(path))
}

fn any_file_str(paths: &[String], names: &[&str]) -> bool {
    paths.iter().any(|path| names.contains(&path.as_str()))
}

fn is_workflow_router_path(path: &str) -> bool {
    starts(path, &[".github/workflows/"]) || contains_path(workflow_router_paths(), path)
}

fn contains_path(paths: &[&str], needle: &str) -> bool {
    paths.contains(&needle)
}

fn workflow_router_paths() -> &'static [&'static str] {
    &[
        "lefthook.yml",
        "scripts/ci/changed_paths.py",
        "tests/ci_changed_paths.rs",
        "tests/workflow_shapes.rs",
        "xtask/src/main.rs",
        "xtask/src/pre_push.rs",
    ]
}

fn rust_ci_helper_scripts() -> &'static [&'static str] {
    &[
        "scripts/cargo_test_filter_guard.py",
        "scripts/check_lefthook_pre_commit_speed.py",
        "scripts/check_shell_completions.sh",
        "scripts/enforce_monoliths.py",
        "scripts/generate_mcp_schema_doc.py",
        "scripts/test-ask-quality-regressions.sh",
        "scripts/test-mcp-oauth-protection.sh",
        "scripts/test-mcp-tools-mcporter.sh",
    ]
}

fn mcp_ci_helper_scripts() -> &'static [&'static str] {
    &[
        "scripts/generate_mcp_schema_doc.py",
        "scripts/test-mcp-oauth-protection.sh",
        "scripts/test-mcp-tools-mcporter.sh",
    ]
}

fn doc_ci_helper_scripts() -> &'static [&'static str] {
    &["scripts/check_aurora_primitive_inventory.py"]
}

#[cfg(test)]
mod tests;

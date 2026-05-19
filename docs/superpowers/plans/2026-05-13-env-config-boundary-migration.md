# Axon Env Config Boundary Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce Axon's env surface so `.env` contains only secrets, endpoint URLs, auth/runtime bootstrap, trusted local overrides, and explicitly classified Docker Compose interpolation; move legitimate non-secret tuning to `config.toml` and delete the rest.

**Architecture:** Treat this as a source-boundary migration, not a template cleanup. First create a checked-in classification matrix and drift checker, then make config parsing/setup/deploy use that same classification, then shrink templates/docs/live env behavior. Compatibility handling is centralized so parser warnings, setup migration, docs, and tests cannot diverge.

**Tech Stack:** Rust 2024, Clap, dotenvy, toml/toml_edit, Docker Compose, Bash helper scripts, Beads epic `axon_rust-ztqd`.

---

## Scope And Order

This plan covers all child beads under `axon_rust-ztqd`:

- `axon_rust-ztqd.1`: source-derived env inventory and classification.
- `axon_rust-ztqd.2`: TOML schema and parser changes.
- `axon_rust-ztqd.3`: `.env.example` and live `~/.axon/.env` migration behavior.
- `axon_rust-ztqd.4`: obsolete env deletion and compatibility shims.
- `axon_rust-ztqd.5`: docs/setup/deploy rewrite.
- `axon_rust-ztqd.6`: end-to-end verification.

Do not start by editing `.env.example`. The first working artifact is the migration matrix and drift checker. That matrix drives every later task.

## File Structure

Create:

- `docs/config/env-migration-matrix.toml`: checked-in classification matrix. No secret values.
- `scripts/check-env-config-boundary.py`: drift checker for env reads, templates, docs, TOML schema, setup writers, and Compose interpolation.
- `src/core/config/parse/env_registry.rs`: central env metadata and compatibility/deprecation registry.
- `src/services/setup/local/env_migration.rs`: explicit backup-backed migration/prune mode for env files.
- `tests/env_config_boundary.rs`: drift and classification tests.
- `tests/setup_env_migration.rs`: temp-HOME tests for explicit migration/prune and non-destructive repair.

Modify:

- `src/core/config/parse.rs`: expose `env_registry` module.
- `src/core/config/parse/toml_config.rs`: stage `[services]` URL deprecation/migration behavior.
- `src/core/config/parse/build_config/config_literal.rs`: stop silently resolving service URLs from TOML.
- `src/core/config/parse/tuning.rs`: only keep env overrides listed by the registry.
- `src/services/setup/local/env.rs`: keep normal repair additive/non-destructive.
- `src/services/setup/local.rs`: add explicit migration/prune command path wiring.
- `src/services/setup/config_store.rs`: stop writing service URLs into TOML.
- `src/services/setup/deploy.rs`: stop using broad publish string replacement and stop calling `write_remote_service_urls`.
- `src/services/setup/assets.rs`: shrink generated env asset and align compose keys.
- `src/core/config/cli.rs`: add an explicit setup migration/prune flag or subcommand option.
- `docker-compose.yaml`: resolve `AXON_MCP_HTTP_PUBLISH` default and Compose env classification.
- `.env.example`: reduce to <=30 lines including comments.
- `config.example.toml`: remove `[services]`; document only non-secret tuning.
- `docs/CONFIG.md`, `docs/mcp/ENV.md`, `docs/auth/MCP-AUTH.md`, `docs/SETUP.md`, `docs/DEPLOYMENT.md`: rewrite around split boundary and redacted secret checks.
- `tests/config_home_pipeline.rs`, `tests/compose_env_contract.rs`, `tests/setup_check_cli.rs`: update contract tests.

Do not modify live `~/.axon/.env` until the explicit migration task. When that task runs, create a timestamped backup first and never print secret values.

---

### Task 1: Add The Migration Matrix And Drift Checker

**Files:**
- Create: `docs/config/env-migration-matrix.toml`
- Create: `scripts/check-env-config-boundary.py`
- Create: `tests/env_config_boundary.rs`
- Modify: `Cargo.toml` only if a new dev dependency is required; prefer not to add one.

- [ ] **Step 1: Create the initial classification matrix**

Create `docs/config/env-migration-matrix.toml` with the categories needed by the beads. Start with known high-risk keys and the schema shape; later tasks expand the entries until the drift checker passes.

```toml
# Axon env/config migration matrix.
# No secret values belong in this file.

[metadata]
version = 1
source = "axon_rust-ztqd"

[[env]]
key = "QDRANT_URL"
classification = "keep-env"
runtime_placement = "both"
surfaces = ["src", "env-template", "docs", "compose-env-file"]
secret = false
url_or_path = true
container_allowed = true
toml_destination = ""
compatibility = "canonical"
reason = "Qdrant endpoint URL. URLs stay in env/CLI, not TOML."

[[env]]
key = "TEI_URL"
classification = "keep-env"
runtime_placement = "both"
surfaces = ["src", "env-template", "docs", "compose-env-file"]
secret = false
url_or_path = true
container_allowed = true
toml_destination = ""
compatibility = "canonical"
reason = "TEI endpoint URL. URLs stay in env/CLI, not TOML."

[[env]]
key = "AXON_CHROME_REMOTE_URL"
classification = "keep-env"
runtime_placement = "both"
surfaces = ["src", "env-template", "docs", "compose-env-file"]
secret = false
url_or_path = true
container_allowed = true
toml_destination = ""
compatibility = "canonical"
reason = "Chrome/CDP endpoint URL. URLs stay in env/CLI, not TOML."

[[env]]
key = "AXON_MCP_HTTP_TOKEN"
classification = "keep-env"
runtime_placement = "container-required"
surfaces = ["src", "env-template", "docs", "setup"]
secret = true
url_or_path = false
container_allowed = true
toml_destination = ""
compatibility = "canonical"
reason = "Runtime auth secret for MCP/action HTTP surfaces."

[[env]]
key = "AXON_ENV_FILE"
classification = "trusted-operator-bootstrap"
runtime_placement = "host-only"
surfaces = ["src", "scripts", "docs"]
secret = false
url_or_path = true
container_allowed = false
toml_destination = ""
compatibility = "advanced"
reason = "Explicit env-file override. It can shadow ~/.axon/.env and must be detected before migration."

[[env]]
key = "AXON_CONFIG_PATH"
classification = "trusted-operator-bootstrap"
runtime_placement = "host-only"
surfaces = ["src", "setup", "docs"]
secret = false
url_or_path = true
container_allowed = false
toml_destination = ""
compatibility = "advanced"
reason = "Explicit config override. Trusted local operator input only."

[[env]]
key = "AXON_MCP_HTTP_PUBLISH"
classification = "compose-env"
runtime_placement = "compose-interpolation"
surfaces = ["compose", "env-template", "docs", "setup"]
secret = false
url_or_path = false
container_allowed = false
toml_destination = ""
compatibility = "canonical"
reason = "Docker Compose host publish address. Compose cannot read TOML."

[[env]]
key = "TEI_MAX_CLIENT_BATCH_SIZE"
classification = "move-toml"
runtime_placement = "not-runtime"
surfaces = ["src", "docs", "config-example"]
secret = false
url_or_path = false
container_allowed = false
toml_destination = "tei.max-client-batch-size"
compatibility = "warn-env-override"
reason = "TEI client request batch size. Distinguish from TEI server argv in Compose."
```

- [ ] **Step 2: Write the drift checker**

Create `scripts/check-env-config-boundary.py`.

```python
#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs/config/env-migration-matrix.toml"

ENV_RE = re.compile(r"\b[A-Z][A-Z0-9_]{2,}\b")
SCAN_GLOBS = [
    "src/**/*.rs",
    "tests/**/*.rs",
    "scripts/**",
    "docker-compose.yaml",
    ".env.example",
    "config.example.toml",
    "docs/CONFIG.md",
    "docs/mcp/ENV.md",
    "docs/auth/MCP-AUTH.md",
    "docs/SETUP.md",
    "docs/DEPLOYMENT.md",
]

ALLOWED_NON_ENV_TOKENS = {
    "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS",
    "JSON", "HTML", "HTTP", "HTTPS", "URL", "URI", "CLI", "MCP",
    "TEI", "RAG", "QDRANT", "CDP", "GPU", "CPU", "TOML", "HOME",
}


def load_matrix() -> dict[str, dict[str, object]]:
    data = tomllib.loads(MATRIX.read_text())
    entries = data.get("env", [])
    by_key: dict[str, dict[str, object]] = {}
    for entry in entries:
        key = entry["key"]
        if key in by_key:
            raise SystemExit(f"duplicate matrix key: {key}")
        by_key[key] = entry
    return by_key


def scan_env_tokens() -> dict[str, set[str]]:
    found: dict[str, set[str]] = {}
    for pattern in SCAN_GLOBS:
        for path in ROOT.glob(pattern):
            if path.is_dir():
                continue
            try:
                text = path.read_text(errors="ignore")
            except UnicodeDecodeError:
                continue
            for token in ENV_RE.findall(text):
                if token in ALLOWED_NON_ENV_TOKENS:
                    continue
                if token.startswith(("AXON_", "OPENAI_", "TEI_", "QDRANT_", "TAVILY_", "GITHUB_", "REDDIT_", "HF_", "GEMINI_", "CUDA_", "NVIDIA_")):
                    found.setdefault(token, set()).add(str(path.relative_to(ROOT)))
    return found


def main() -> int:
    matrix = load_matrix()
    found = scan_env_tokens()
    missing = sorted(set(found) - set(matrix))
    stale = sorted(key for key in matrix if key not in found and matrix[key].get("compatibility") != "historical")

    errors: list[str] = []
    if missing:
        errors.append("Env keys missing from migration matrix:")
        errors.extend(f"  {key}: {sorted(found[key])[:6]}" for key in missing)
    if stale:
        errors.append("Matrix keys not found in active scan:")
        errors.extend(f"  {key}" for key in stale)

    for key, entry in sorted(matrix.items()):
        classification = entry.get("classification")
        placement = entry.get("runtime_placement")
        if classification not in {
            "keep-env", "compose-env", "move-toml", "delete", "hard-default",
            "trusted-operator-bootstrap", "compatibility-shim", "external/test-only",
        }:
            errors.append(f"{key}: invalid classification {classification!r}")
        if placement not in {
            "host-only", "container-required", "compose-interpolation", "both", "not-runtime",
        }:
            errors.append(f"{key}: invalid runtime_placement {placement!r}")
        if classification == "move-toml" and not entry.get("toml_destination"):
            errors.append(f"{key}: move-toml requires toml_destination")
        if classification in {"keep-env", "compose-env", "trusted-operator-bootstrap"} and entry.get("toml_destination"):
            errors.append(f"{key}: env/bootstrap key must not have toml_destination")

    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"env/config boundary ok: {len(matrix)} classified keys")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 3: Make the checker executable**

Run:

```bash
chmod +x scripts/check-env-config-boundary.py
```

- [ ] **Step 4: Add the Rust integration test**

Create `tests/env_config_boundary.rs`.

```rust
use std::process::Command;

#[test]
fn env_config_boundary_matrix_is_current() {
    let output = Command::new("python3")
        .arg("scripts/check-env-config-boundary.py")
        .output()
        .expect("run env boundary checker");

    assert!(
        output.status.success(),
        "checker failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
```

- [ ] **Step 5: Run the checker and expand the matrix until it fails only for real unknowns**

Run:

```bash
python3 scripts/check-env-config-boundary.py
```

Expected first result: FAIL, listing missing keys from source/docs/templates.

Expand `docs/config/env-migration-matrix.toml` with every listed key. Do not add secret values. Use these classifications exactly:

```toml
classification = "keep-env"                  # secrets, URLs, auth/runtime state
classification = "compose-env"               # Docker Compose interpolation only
classification = "move-toml"                 # durable non-secret operator tuning
classification = "delete"                    # obsolete/stale/duplicated key
classification = "hard-default"              # internal tuning, no user config
classification = "trusted-operator-bootstrap" # local path/config bootstrap
classification = "compatibility-shim"        # temporary legacy behavior
classification = "external/test-only"        # dev/test keys not in production template
```

- [ ] **Step 6: Run the test**

Run:

```bash
cargo test --test env_config_boundary
```

Expected: PASS after every active key is classified.

- [ ] **Step 7: Commit**

```bash
git add docs/config/env-migration-matrix.toml scripts/check-env-config-boundary.py tests/env_config_boundary.rs
git commit -m "chore(config): classify env boundary"
```

---

### Task 2: Add Central Env Registry And Compatibility Metadata

**Files:**
- Create: `src/core/config/parse/env_registry.rs`
- Modify: `src/core/config/parse.rs`
- Test: `src/core/config/parse/env_registry.rs`

- [ ] **Step 1: Add the registry module declaration**

Modify `src/core/config/parse.rs` to include:

```rust
pub(crate) mod env_registry;
```

- [ ] **Step 2: Create the registry**

Create `src/core/config/parse/env_registry.rs`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnvClassification {
    KeepEnv,
    ComposeEnv,
    MoveToml,
    Delete,
    HardDefault,
    TrustedOperatorBootstrap,
    CompatibilityShim,
    ExternalTestOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimePlacement {
    HostOnly,
    ContainerRequired,
    ComposeInterpolation,
    Both,
    NotRuntime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyBehavior {
    Canonical,
    WarnEnvOverride,
    WarnAndIgnore,
    FailWithReplacement,
    HardIgnore,
    DeleteOnMigration,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct EnvKeySpec {
    pub key: &'static str,
    pub classification: EnvClassification,
    pub placement: RuntimePlacement,
    pub toml_destination: Option<&'static str>,
    pub legacy_behavior: LegacyBehavior,
    pub secret: bool,
}

pub(crate) const ENV_KEY_SPECS: &[EnvKeySpec] = &[
    EnvKeySpec {
        key: "QDRANT_URL",
        classification: EnvClassification::KeepEnv,
        placement: RuntimePlacement::Both,
        toml_destination: None,
        legacy_behavior: LegacyBehavior::Canonical,
        secret: false,
    },
    EnvKeySpec {
        key: "TEI_URL",
        classification: EnvClassification::KeepEnv,
        placement: RuntimePlacement::Both,
        toml_destination: None,
        legacy_behavior: LegacyBehavior::Canonical,
        secret: false,
    },
    EnvKeySpec {
        key: "AXON_CHROME_REMOTE_URL",
        classification: EnvClassification::KeepEnv,
        placement: RuntimePlacement::Both,
        toml_destination: None,
        legacy_behavior: LegacyBehavior::Canonical,
        secret: false,
    },
    EnvKeySpec {
        key: "AXON_MCP_HTTP_TOKEN",
        classification: EnvClassification::KeepEnv,
        placement: RuntimePlacement::ContainerRequired,
        toml_destination: None,
        legacy_behavior: LegacyBehavior::Canonical,
        secret: true,
    },
    EnvKeySpec {
        key: "AXON_ENV_FILE",
        classification: EnvClassification::TrustedOperatorBootstrap,
        placement: RuntimePlacement::HostOnly,
        toml_destination: None,
        legacy_behavior: LegacyBehavior::Canonical,
        secret: false,
    },
    EnvKeySpec {
        key: "AXON_CONFIG_PATH",
        classification: EnvClassification::TrustedOperatorBootstrap,
        placement: RuntimePlacement::HostOnly,
        toml_destination: None,
        legacy_behavior: LegacyBehavior::Canonical,
        secret: false,
    },
    EnvKeySpec {
        key: "TEI_MAX_CLIENT_BATCH_SIZE",
        classification: EnvClassification::MoveToml,
        placement: RuntimePlacement::NotRuntime,
        toml_destination: Some("tei.max-client-batch-size"),
        legacy_behavior: LegacyBehavior::WarnEnvOverride,
        secret: false,
    },
    EnvKeySpec {
        key: "AXON_BATCH_QUEUE",
        classification: EnvClassification::Delete,
        placement: RuntimePlacement::NotRuntime,
        toml_destination: None,
        legacy_behavior: LegacyBehavior::DeleteOnMigration,
        secret: false,
    },
];

pub(crate) fn spec_for(key: &str) -> Option<&'static EnvKeySpec> {
    ENV_KEY_SPECS.iter().find(|spec| spec.key == key)
}

pub(crate) fn warn_legacy_env_override(key: &str, destination: &str) {
    eprintln!(
        "axon: warning: {key} is deprecated for non-secret tuning; move this value to config.toml [{destination}]"
    );
}

pub(crate) fn is_allowed_env_template_key(key: &str) -> bool {
    spec_for(key).is_some_and(|spec| {
        matches!(
            spec.classification,
            EnvClassification::KeepEnv
                | EnvClassification::ComposeEnv
                | EnvClassification::TrustedOperatorBootstrap
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_urls_are_env_not_toml() {
        for key in ["QDRANT_URL", "TEI_URL", "AXON_CHROME_REMOTE_URL"] {
            let spec = spec_for(key).expect("registered key");
            assert_eq!(spec.classification, EnvClassification::KeepEnv);
            assert_eq!(spec.toml_destination, None);
        }
    }

    #[test]
    fn moved_tuning_has_toml_destination() {
        for spec in ENV_KEY_SPECS {
            if spec.classification == EnvClassification::MoveToml {
                assert!(
                    spec.toml_destination.is_some(),
                    "{} is move-toml without destination",
                    spec.key
                );
            }
        }
    }
}
```

- [ ] **Step 3: Add all matrix keys to the registry**

For every key in `docs/config/env-migration-matrix.toml`, add an `EnvKeySpec`. The registry does not need to duplicate every comment from TOML, but it must carry behavior needed at runtime.

- [ ] **Step 4: Run registry tests**

Run:

```bash
cargo test env_registry
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/core/config/parse.rs src/core/config/parse/env_registry.rs
git commit -m "feat(config): add env boundary registry"
```

---

### Task 3: Stage `[services]` TOML URL Removal Safely

**Files:**
- Modify: `src/core/config/parse/toml_config.rs`
- Modify: `src/core/config/parse/build_config/config_literal.rs`
- Modify: `tests/config_home_pipeline.rs`
- Modify: `config.example.toml`

- [ ] **Step 1: Write tests for legacy `[services]` URL behavior**

Add tests in `src/core/config/parse/toml_config.rs`:

```rust
#[test]
fn legacy_services_urls_parse_for_migration_only() {
    let cfg = load_toml_config_from_str(
        r#"
        [services]
        qdrant-url = "http://127.0.0.1:53333"
        tei-url = "http://127.0.0.1:52000"
        chrome-remote-url = "http://127.0.0.1:6000"
        "#,
    )
    .expect("legacy services section should parse for migration");

    assert_eq!(
        cfg.services.qdrant_url.as_deref(),
        Some("http://127.0.0.1:53333")
    );
    assert_eq!(
        cfg.services.tei_url.as_deref(),
        Some("http://127.0.0.1:52000")
    );
    assert_eq!(
        cfg.services.chrome_remote_url.as_deref(),
        Some("http://127.0.0.1:6000")
    );
}
```

Add a build-config test near existing priority-chain service URL tests:

```rust
#[test]
fn toml_services_urls_do_not_satisfy_required_service_urls() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = tempfile::NamedTempFile::new().unwrap();
    writeln!(
        f,
        "[services]\nqdrant-url = \"http://127.0.0.1:53333\"\ntei-url = \"http://127.0.0.1:52000\""
    )
    .unwrap();

    with_env_saved(&["AXON_CONFIG_PATH", "QDRANT_URL", "TEI_URL"], || unsafe {
        std::env::set_var("AXON_CONFIG_PATH", f.path());
        std::env::remove_var("QDRANT_URL");
        std::env::remove_var("TEI_URL");
        let err = into_config(cli_with_services(&["status"])).unwrap_err();
        assert!(
            err.contains("TEI_URL environment variable is required")
                || err.contains("QDRANT_URL environment variable is required"),
            "unexpected error: {err}"
        );
    });
}
```

- [ ] **Step 2: Run tests to see failure**

Run:

```bash
cargo test toml_services_urls_do_not_satisfy_required_service_urls legacy_services_urls_parse_for_migration_only
```

Expected: FAIL because `config_literal.rs` still falls back to `toml.services.*`.

- [ ] **Step 3: Stop using TOML service URLs for runtime resolution**

Modify `src/core/config/parse/build_config/config_literal.rs`.

Change Chrome resolution from:

```rust
.or_else(|| inputs.toml.services.chrome_remote_url.clone())
```

to:

```rust
```

Delete that fallback entirely so the chain is:

```rust
cfg.chrome_remote_url = g
    .chrome_remote_url
    .clone()
    .or_else(|| env::var("AXON_CHROME_REMOTE_URL").ok())
    .map(normalize_local_service_url);
```

Change `resolve_tei_url` and `resolve_qdrant_url` so they do not read `toml.services`:

```rust
fn resolve_tei_url(global: &GlobalArgs, _toml: &TomlConfig) -> Result<String, String> {
    global
        .tei_url
        .clone()
        .or_else(|| env::var("TEI_URL").ok())
        .ok_or_else(|| {
            "TEI_URL environment variable is required (or pass --tei-url). Service URLs are not read from config.toml; move legacy [services].tei-url to .env.".to_string()
        })
        .map(normalize_local_service_url)
}

fn resolve_qdrant_url(global: &GlobalArgs, _toml: &TomlConfig) -> Result<String, String> {
    global
        .qdrant_url
        .clone()
        .or_else(|| env::var("QDRANT_URL").ok())
        .ok_or_else(|| {
            "QDRANT_URL environment variable is required (or pass --qdrant-url). Service URLs are not read from config.toml; move legacy [services].qdrant-url to .env.".to_string()
        })
        .map(normalize_local_service_url)
}
```

- [ ] **Step 4: Keep legacy parsing but mark it migration-only**

In `src/core/config/parse/toml_config.rs`, keep `TomlServicesSection` for now, but update comments:

```rust
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlServicesSection {
    /// Legacy migration-only field. Runtime resolution ignores this value.
    pub qdrant_url: Option<String>,
    /// Legacy migration-only field. Runtime resolution ignores this value.
    pub tei_url: Option<String>,
    /// Legacy migration-only field. Runtime resolution ignores this value.
    pub chrome_remote_url: Option<String>,
}
```

- [ ] **Step 5: Remove `[services]` from `config.example.toml`**

Delete the `[services]` section and replace it with this note near the top:

```toml
# Service endpoint URLs are intentionally not configured here.
# Put QDRANT_URL, TEI_URL, and AXON_CHROME_REMOTE_URL in ~/.axon/.env
# or pass --qdrant-url / --tei-url / --chrome-remote-url on the CLI.
```

- [ ] **Step 6: Run targeted tests**

Run:

```bash
cargo test toml_services_urls_do_not_satisfy_required_service_urls legacy_services_urls_parse_for_migration_only
cargo test --test config_home_pipeline
```

Expected: PASS after updating any test that currently blesses `[services] tei-url`.

- [ ] **Step 7: Commit**

```bash
git add src/core/config/parse/toml_config.rs src/core/config/parse/build_config/config_literal.rs tests/config_home_pipeline.rs config.example.toml
git commit -m "fix(config): stop resolving service URLs from TOML"
```

---

### Task 4: Make Setup Repair Non-Destructive And Add Explicit Env Migration

**Files:**
- Create: `src/services/setup/local/env_migration.rs`
- Modify: `src/services/setup/local.rs`
- Modify: `src/services/setup/local/env.rs`
- Modify: `src/core/config/cli.rs`
- Test: `tests/setup_env_migration.rs`

- [ ] **Step 1: Write tests for non-destructive repair and explicit migration**

Create `tests/setup_env_migration.rs`.

```rust
use std::process::Command;

#[test]
fn setup_repair_preserves_unknown_keys_without_pruning() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let axon_home = home.join(".axon");
    std::fs::create_dir_all(&axon_home).unwrap();
    std::fs::write(
        axon_home.join(".env"),
        "TAVILY_API_KEY=keep\nUNKNOWN_LOCAL_SECRET=keep-too\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_axon"))
        .args(["setup", "repair", "--json"])
        .env("HOME", &home)
        .env_remove("AXON_ENV_FILE")
        .output()
        .expect("run setup repair");

    assert!(
        output.status.success(),
        "setup repair failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(axon_home.join(".env")).unwrap();
    assert!(raw.contains("UNKNOWN_LOCAL_SECRET=keep-too"));
}

#[test]
fn explicit_env_migration_backs_up_and_reports_without_values() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let axon_home = home.join(".axon");
    std::fs::create_dir_all(&axon_home).unwrap();
    std::fs::write(
        axon_home.join(".env"),
        "AXON_BATCH_QUEUE=old\nTAVILY_API_KEY=secret-value\nTEI_MAX_CLIENT_BATCH_SIZE=32\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_axon"))
        .args(["setup", "repair", "--migrate-env", "--json"])
        .env("HOME", &home)
        .env_remove("AXON_ENV_FILE")
        .output()
        .expect("run setup migrate");

    assert!(
        output.status.success(),
        "setup migrate failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backup"));
    assert!(!stdout.contains("secret-value"));
    let backups = std::fs::read_dir(&axon_home)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".env.backup."))
        .count();
    assert_eq!(backups, 1);
}
```

- [ ] **Step 2: Run tests to see failure**

Run:

```bash
cargo test --test setup_env_migration
```

Expected: FAIL because `--migrate-env` does not exist.

- [ ] **Step 3: Add CLI flag**

Modify the setup command options in `src/core/config/cli.rs` so `setup repair` accepts:

```rust
/// Prune/migrate ~/.axon/.env using the env migration matrix. Creates a backup first.
#[arg(long)]
pub migrate_env: bool,
```

Use the existing setup command struct shape. If setup options are represented as an enum variant field, add `migrate_env: bool` to that variant and thread it into `src/services/setup/local.rs`.

- [ ] **Step 4: Add migration result types**

Create `src/services/setup/local/env_migration.rs`.

```rust
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub(super) struct EnvMigrationReport {
    pub backup_path: PathBuf,
    pub retained_env: usize,
    pub moved_toml: usize,
    pub compose_env: usize,
    pub deleted: usize,
    pub hard_defaulted: usize,
    pub compatibility_shims: usize,
    pub preserved_unclassified: usize,
}

pub(super) fn migrate_env_file(path: &Path) -> io::Result<EnvMigrationReport> {
    if let Ok(explicit) = std::env::var("AXON_ENV_FILE") {
        if !explicit.trim().is_empty() && Path::new(explicit.trim()) != path {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "AXON_ENV_FILE is set to {}; migrate that effective env file or unset AXON_ENV_FILE before migrating {}",
                    explicit.trim(),
                    path.display()
                ),
            ));
        }
    }

    let raw = std::fs::read_to_string(path)?;
    let backup_path = backup_env(path)?;
    let parsed = parse_simple_env(&raw);

    let mut retained = BTreeMap::new();
    let mut report = EnvMigrationReport {
        backup_path,
        ..EnvMigrationReport::default()
    };

    for (key, value) in parsed {
        match key.as_str() {
            "QDRANT_URL" | "TEI_URL" | "AXON_CHROME_REMOTE_URL" | "AXON_MCP_HTTP_TOKEN"
            | "TAVILY_API_KEY" | "GITHUB_TOKEN" | "REDDIT_CLIENT_ID" | "REDDIT_CLIENT_SECRET"
            | "HF_TOKEN" | "AXON_MCP_AUTH_MODE" | "AXON_MCP_PUBLIC_URL"
            | "AXON_MCP_GOOGLE_CLIENT_ID" | "AXON_MCP_GOOGLE_CLIENT_SECRET"
            | "AXON_MCP_AUTH_ADMIN_EMAIL" | "AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS"
            | "AXON_MCP_ALLOWED_ORIGINS" => {
                retained.insert(key, value);
                report.retained_env += 1;
            }
            "AXON_MCP_HTTP_PUBLISH" | "AXON_IMAGE" | "GEMINI_HOME" | "TEI_EMBEDDING_MODEL"
            | "TEI_HTTP_PORT" | "NVIDIA_VISIBLE_DEVICES" | "CUDA_VISIBLE_DEVICES" => {
                retained.insert(key, value);
                report.compose_env += 1;
            }
            "TEI_MAX_CLIENT_BATCH_SIZE" | "TEI_MAX_RETRIES" | "TEI_REQUEST_TIMEOUT_MS"
            | "AXON_INGEST_LANES" | "AXON_EMBED_DOC_TIMEOUT_SECS"
            | "AXON_MAX_PENDING_CRAWL_JOBS" => {
                report.moved_toml += 1;
            }
            "AXON_BATCH_QUEUE" | "AXON_CRAWL_QUEUE" | "AXON_EMBED_QUEUE"
            | "AXON_EXTRACT_QUEUE" | "AXON_INGEST_QUEUE" => {
                report.deleted += 1;
            }
            _ => {
                report.preserved_unclassified += 1;
            }
        }
    }

    write_minimal_env(path, &retained)?;
    Ok(report)
}

fn backup_env(path: &Path) -> io::Result<PathBuf> {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let backup = path.with_file_name(format!(".env.backup.{stamp}"));
    std::fs::copy(path, &backup)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(backup)
}

fn parse_simple_env(raw: &str) -> BTreeMap<String, String> {
    raw.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            let (key, value) = trimmed.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

fn write_minimal_env(path: &Path, env: &BTreeMap<String, String>) -> io::Result<()> {
    let mut out = String::from("# Axon runtime env: secrets, URLs, auth, bootstrap, compose interpolation only.\n");
    for (key, value) in env {
        if value.contains(['\n', '\r']) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{key} contains a newline and cannot be safely written"),
            ));
        }
        out.push_str(key);
        out.push('=');
        out.push_str(value);
        out.push('\n');
    }
    std::fs::write(path, out)
}
```

If the repo does not already depend on `chrono`, use `std::time::SystemTime` instead of adding a dependency.

- [ ] **Step 5: Wire migration mode from setup repair**

In `src/services/setup/local.rs`, call `env_migration::migrate_env_file(env_path)` only when `migrate_env` is true. Normal repair continues calling `env::ensure_env_file(env_path)` and preserving unknown keys.

Report only counts and backup path. Do not print values.

- [ ] **Step 6: Run targeted tests**

Run:

```bash
cargo test --test setup_env_migration
cargo test setup_repair
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/core/config/cli.rs src/services/setup/local.rs src/services/setup/local/env.rs src/services/setup/local/env_migration.rs tests/setup_env_migration.rs
git commit -m "feat(setup): add explicit env migration mode"
```

---

### Task 5: Stop Remote Deploy From Writing Service URLs To TOML

**Files:**
- Modify: `src/services/setup/config_store.rs`
- Modify: `src/services/setup/deploy.rs`
- Test: `src/services/setup/config_store.rs`

- [ ] **Step 1: Replace the config writer test**

In `src/services/setup/config_store.rs`, replace `write_remote_service_urls_honors_axon_config_path` with:

```rust
#[allow(unsafe_code)]
#[test]
fn write_remote_runtime_env_does_not_write_service_urls_to_toml() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("custom.toml");
    let env_path = dir.path().join(".env");
    let previous = std::env::var_os("AXON_CONFIG_PATH");
    unsafe {
        std::env::set_var("AXON_CONFIG_PATH", &config_path);
    }

    let written = write_remote_runtime_env(
        &env_path,
        "http://127.0.0.1:53333",
        "http://127.0.0.1:52000",
        "http://127.0.0.1:6000",
    )
    .unwrap();

    assert_eq!(written, env_path);
    let env_raw = std::fs::read_to_string(&written).unwrap();
    assert!(env_raw.contains("QDRANT_URL=http://127.0.0.1:53333"));
    assert!(env_raw.contains("TEI_URL=http://127.0.0.1:52000"));
    assert!(env_raw.contains("AXON_CHROME_REMOTE_URL=http://127.0.0.1:6000"));

    let config_raw = std::fs::read_to_string(&config_path).unwrap_or_default();
    assert!(!config_raw.contains("[services]"));
    assert!(!config_raw.contains("qdrant-url"));
    assert!(!config_raw.contains("tei-url"));

    unsafe {
        if let Some(previous) = previous {
            std::env::set_var("AXON_CONFIG_PATH", previous);
        } else {
            std::env::remove_var("AXON_CONFIG_PATH");
        }
    }
}
```

- [ ] **Step 2: Run test to see failure**

Run:

```bash
cargo test write_remote_runtime_env_does_not_write_service_urls_to_toml
```

Expected: FAIL because `write_remote_runtime_env` does not exist.

- [ ] **Step 3: Add env writer and remove service URL TOML writer**

In `src/services/setup/config_store.rs`, replace `write_remote_service_urls` with:

```rust
pub fn write_remote_runtime_env(
    env_path: &Path,
    qdrant_url: &str,
    tei_url: &str,
    chrome_remote_url: &str,
) -> io::Result<PathBuf> {
    let parent = env_path.parent().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            format!("env path '{}' has no parent", env_path.display()),
        )
    })?;
    std::fs::create_dir_all(parent)?;
    let contents = format!(
        "# Axon remote runtime env.\nQDRANT_URL={qdrant_url}\nTEI_URL={tei_url}\nAXON_CHROME_REMOTE_URL={chrome_remote_url}\n"
    );
    write_private_file(env_path, &contents)?;
    Ok(env_path.to_path_buf())
}
```

Delete or stop exporting `write_remote_service_urls`.

- [ ] **Step 4: Update deploy**

In `src/services/setup/deploy.rs`, replace:

```rust
let config_path = write_remote_service_urls(&qdrant_url, &tei_url, &chrome_remote_url)?;
```

with:

```rust
let env_path = std::path::PathBuf::from(".env");
let runtime_env_path =
    write_remote_runtime_env(&env_path, &qdrant_url, &tei_url, &chrome_remote_url)?;
```

Update status/report naming from config path to env path.

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test write_remote_runtime_env_does_not_write_service_urls_to_toml
cargo check --bin axon
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/services/setup/config_store.rs src/services/setup/deploy.rs
git commit -m "fix(setup): write remote service URLs to env"
```

---

### Task 6: Resolve Compose Boundary And Shrink `.env.example`

**Files:**
- Modify: `docker-compose.yaml`
- Modify: `.env.example`
- Modify: `src/services/setup/assets.rs`
- Modify: `tests/compose_env_contract.rs`

- [ ] **Step 1: Pick the publish default**

Use loopback as the default in templates and Compose fallback unless the user intentionally sets `AXON_MCP_HTTP_PUBLISH=0.0.0.0:8001`.

In `docker-compose.yaml`, change:

```yaml
- "${AXON_MCP_HTTP_PUBLISH:-0.0.0.0:8001}:8001"
```

to:

```yaml
- "${AXON_MCP_HTTP_PUBLISH:-127.0.0.1:8001}:8001"
```

- [ ] **Step 2: Separate TEI client and server batch naming**

In `docker-compose.yaml`, replace server arg use of `TEI_MAX_CLIENT_BATCH_SIZE` with a compose-specific key:

```yaml
- "--max-client-batch-size"
- "${TEI_SERVER_MAX_CLIENT_BATCH_SIZE:-96}"
```

In the matrix, classify `TEI_SERVER_MAX_CLIENT_BATCH_SIZE` as `compose-env` and keep `TEI_MAX_CLIENT_BATCH_SIZE` as `move-toml`/compatibility shim for the client.

- [ ] **Step 3: Shrink `.env.example`**

Rewrite `.env.example` to this shape, keeping total lines <=30:

```dotenv
# Axon runtime env: secrets, endpoint URLs, auth, bootstrap, compose only.
AXON_DATA_DIR=
QDRANT_URL=http://127.0.0.1:53333
TEI_URL=http://127.0.0.1:52000
AXON_CHROME_REMOTE_URL=http://127.0.0.1:6000
AXON_SERVER_URL=http://127.0.0.1:8001

AXON_MCP_HTTP_TOKEN=
AXON_MCP_AUTH_MODE=bearer
AXON_MCP_PUBLIC_URL=
AXON_MCP_GOOGLE_CLIENT_ID=
AXON_MCP_GOOGLE_CLIENT_SECRET=
AXON_MCP_AUTH_ADMIN_EMAIL=
AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS=
AXON_MCP_ALLOWED_ORIGINS=

TAVILY_API_KEY=
GITHUB_TOKEN=
REDDIT_CLIENT_ID=
REDDIT_CLIENT_SECRET=
HF_TOKEN=
GEMINI_HOME=

AXON_IMAGE=
AXON_MCP_HTTP_PUBLISH=127.0.0.1:8001
TEI_EMBEDDING_MODEL=Qwen/Qwen3-Embedding-0.6B
TEI_HTTP_PORT=52000
TEI_SERVER_MAX_CLIENT_BATCH_SIZE=96
NVIDIA_VISIBLE_DEVICES=0
CUDA_VISIBLE_DEVICES=0
```

Count lines:

```bash
wc -l .env.example
```

Expected: `30 .env.example` or fewer.

- [ ] **Step 4: Update setup asset**

Modify `src/services/setup/assets.rs` so generated env uses the same key names and loopback publish default.

- [ ] **Step 5: Update compose contract tests**

In `tests/compose_env_contract.rs`, assert:

```rust
assert!(
    compose.contains("${AXON_MCP_HTTP_PUBLISH:-127.0.0.1:8001}:8001"),
    "compose default publish must be loopback unless explicitly overridden"
);
assert!(
    compose.contains("TEI_SERVER_MAX_CLIENT_BATCH_SIZE"),
    "compose TEI server batch size must not use TEI_MAX_CLIENT_BATCH_SIZE"
);
assert!(
    !compose.contains("${TEI_MAX_CLIENT_BATCH_SIZE:-"),
    "TEI_MAX_CLIENT_BATCH_SIZE is client tuning and must not drive TEI server args"
);
```

- [ ] **Step 6: Run compose tests**

Run:

```bash
cargo test --test compose_env_contract
docker compose --env-file ~/.axon/.env -f docker-compose.yaml config >/tmp/axon-compose-config.out
```

Expected: tests PASS; compose config exits 0.

- [ ] **Step 7: Commit**

```bash
git add docker-compose.yaml .env.example src/services/setup/assets.rs tests/compose_env_contract.rs docs/config/env-migration-matrix.toml
git commit -m "fix(compose): enforce minimal env boundary"
```

---

### Task 7: Move Remaining Legitimate Tuning To TOML Without Per-Request Reads

**Files:**
- Modify: `src/core/config/parse/toml_config.rs`
- Modify: `src/core/config/parse/tuning.rs`
- Modify: `src/core/config/parse/build_config/config_literal.rs`
- Modify: `src/core/config/types/config.rs`
- Modify: `src/jobs/lite/workers.rs`
- Modify: `src/services/context.rs`
- Modify: `src/vector/ops/tei/pipeline.rs`
- Modify: `config.example.toml`
- Test: `src/core/config/parse/build_config/tests/priority_chain/*.rs`

- [ ] **Step 1: Write tests for worker/job tuning through Config**

Add priority-chain tests for:

```rust
#[test]
fn embed_lanes_resolve_from_toml_when_env_absent() {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    writeln!(f, "[workers]\nembed-lanes = 6").unwrap();
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_EMBED_LANES"], || unsafe {
        std::env::set_var("AXON_CONFIG_PATH", f.path());
        std::env::remove_var("AXON_EMBED_LANES");
        let cfg = into_config(cli_with_services(&["status"])).unwrap();
        assert_eq!(cfg.embed_lanes, 6);
    });
}

#[test]
fn env_embed_lanes_override_toml_with_warning_compatibility() {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    writeln!(f, "[workers]\nembed-lanes = 6").unwrap();
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_EMBED_LANES"], || unsafe {
        std::env::set_var("AXON_CONFIG_PATH", f.path());
        std::env::set_var("AXON_EMBED_LANES", "4");
        let cfg = into_config(cli_with_services(&["status"])).unwrap();
        assert_eq!(cfg.embed_lanes, 4);
    });
}
```

- [ ] **Step 2: Add fields to Config**

In `src/core/config/types/config.rs`, add non-secret tuning fields with defaults:

```rust
pub embed_lanes: usize,
pub queue_summary_secs: u64,
pub qdrant_point_buffer: usize,
```

Update `Default`:

```rust
embed_lanes: 2,
queue_summary_secs: 30,
qdrant_point_buffer: 256,
```

Update any inline `Config { .. }` test helper literals called out by compiler errors.

- [ ] **Step 3: Add TOML fields**

In `TomlWorkersSection`:

```rust
pub embed_lanes: Option<usize>,
pub queue_summary_secs: Option<u64>,
pub qdrant_point_buffer: Option<usize>,
```

Add to `config.example.toml`:

```toml
[workers]
# Parallel embed worker lanes. Default: 2, clamp: 1-32.
embed-lanes = 2
# Queue summary interval in seconds. Default: 30, clamp: 5-3600.
queue-summary-secs = 30
# Buffered Qdrant points before flush. Default: 256, clamp: 128-16384.
qdrant-point-buffer = 256
```

- [ ] **Step 4: Resolve values once during config construction**

In `src/core/config/parse/tuning.rs`, add helpers:

```rust
fn embed_lanes(toml: &TomlConfig) -> usize {
    env_usize("AXON_EMBED_LANES")
        .or(toml.workers.embed_lanes)
        .unwrap_or(2)
        .clamp(1, 32)
}

fn queue_summary_secs(toml: &TomlConfig) -> u64 {
    env_u64("AXON_QUEUE_SUMMARY_SECS")
        .or(toml.workers.queue_summary_secs)
        .unwrap_or(30)
        .clamp(5, 3600)
}

fn qdrant_point_buffer(toml: &TomlConfig) -> usize {
    env_usize("AXON_QDRANT_POINT_BUFFER")
        .or(toml.workers.qdrant_point_buffer)
        .unwrap_or(256)
        .clamp(128, 16384)
}
```

Set these in `apply_env_toml_tuning`.

- [ ] **Step 5: Stop direct env reads in runtime loops**

In `src/jobs/lite/workers.rs`, replace:

```rust
let embed_lanes = resolve_lane_count("AXON_EMBED_LANES", 2, 32);
```

with:

```rust
let embed_lanes = cfg.embed_lanes.clamp(1, 32);
```

Thread `cfg: &Config` or `Arc<Config>` into the worker spawn function if it is not already present.

In `src/services/context.rs`, replace direct `AXON_QUEUE_SUMMARY_SECS` parsing with `cfg.queue_summary_secs`.

In `src/vector/ops/tei/pipeline.rs`, replace direct `AXON_QDRANT_POINT_BUFFER` parsing with `cfg.qdrant_point_buffer`. If the pipeline currently does not receive `Config`, pass the integer as a parameter from the caller rather than reading TOML/env inside the loop.

- [ ] **Step 6: Run targeted tests**

Run:

```bash
cargo test embed_lanes_resolve_from_toml_when_env_absent env_embed_lanes_override_toml_with_warning_compatibility
cargo check --bin axon
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/core/config src/jobs/lite/workers.rs src/services/context.rs src/vector/ops/tei/pipeline.rs config.example.toml
git commit -m "feat(config): move worker tuning into TOML"
```

---

### Task 8: Rewrite Docs Around The Split Boundary

**Files:**
- Modify: `docs/CONFIG.md`
- Modify: `docs/mcp/ENV.md`
- Modify: `docs/auth/MCP-AUTH.md`
- Modify: `docs/SETUP.md`
- Modify: `docs/DEPLOYMENT.md`
- Modify: `docs/SECURITY.md`

- [ ] **Step 1: Rewrite `docs/CONFIG.md` top-level boundary**

Use this opening contract:

```markdown
# Axon Configuration

Axon uses two user-editable files under `~/.axon/`:

| File | Owns | Does not own |
|---|---|---|
| `~/.axon/.env` | Secrets, endpoint URLs, auth/runtime bootstrap, trusted local override paths, Docker Compose interpolation | Non-secret tuning knobs |
| `~/.axon/config.toml` | Non-secret tuning defaults for ask/search/TEI client/workers | Secrets, endpoint URLs, OAuth client secrets, bearer tokens |

Priority is:

1. CLI flags
2. Environment variables for secrets, URLs, auth/runtime, bootstrap, and temporary compatibility shims
3. `~/.axon/config.toml` for non-secret tuning
4. Built-in defaults

Service endpoint URLs are intentionally not accepted from `config.toml`.
Use `QDRANT_URL`, `TEI_URL`, `AXON_CHROME_REMOTE_URL`, or CLI flags.
```

- [ ] **Step 2: Replace token-printing commands**

In `docs/auth/MCP-AUTH.md`, replace:

```bash
grep AXON_MCP_HTTP_TOKEN .env
```

with:

```bash
test -n "${AXON_MCP_HTTP_TOKEN:-}" && echo "AXON_MCP_HTTP_TOKEN is set"
```

For file checks, use:

```bash
awk -F= '$1=="AXON_MCP_HTTP_TOKEN" && length($2)>0 { print "AXON_MCP_HTTP_TOKEN is set" }' ~/.axon/.env
```

Do not add commands that print token values.

- [ ] **Step 3: Document explicit migration mode**

Add this to setup docs:

```markdown
`axon setup repair` is non-destructive: it adds missing required runtime keys and repairs blank generated auth tokens, but it does not prune unknown keys.

Use `axon setup repair --migrate-env --json` to perform the env boundary migration. This creates a timestamped backup under `~/.axon/`, moves classified non-secret tuning into `config.toml`, prunes known stale keys, and reports counts without printing secret values.

If `AXON_ENV_FILE` is set, Axon treats that file as the effective env file. The migration refuses to silently rewrite `~/.axon/.env` while runtime is pointed somewhere else.
```

- [ ] **Step 4: Run docs grep checks**

Run:

```bash
rg -n "grep AXON_MCP_HTTP_TOKEN|cat .*\\.env|TOKEN=.*\\$AXON_MCP_HTTP_TOKEN" docs
```

Expected: no unsafe token-printing guidance.

- [ ] **Step 5: Commit**

```bash
git add docs/CONFIG.md docs/mcp/ENV.md docs/auth/MCP-AUTH.md docs/SETUP.md docs/DEPLOYMENT.md docs/SECURITY.md
git commit -m "docs(config): document env and TOML boundary"
```

---

### Task 9: Final Verification And Live User Env Migration

**Files:**
- Modify: live `~/.axon/.env`
- Modify: live `~/.axon/config.toml`
- No source file edits unless verification finds a bug.

- [ ] **Step 1: Run mandatory fast gates**

Run:

```bash
python3 scripts/check-env-config-boundary.py
cargo fmt --check
cargo check --bin axon
cargo test --test env_config_boundary
cargo test --test setup_env_migration
cargo test --test compose_env_contract
cargo test --test config_home_pipeline
cargo test env_registry
docker compose --env-file ~/.axon/.env -f docker-compose.yaml config >/tmp/axon-compose-config.out
```

Expected: all commands exit 0.

- [ ] **Step 2: Capture before counts without values**

Run:

```bash
printf 'before_lines='
wc -l < ~/.axon/.env
printf 'before_keys='
awk -F= '/^[[:space:]]*[A-Za-z_][A-Za-z0-9_]*=/ { count++ } END { print count+0 }' ~/.axon/.env
```

Record counts only.

- [ ] **Step 3: Run explicit migration**

Run:

```bash
./target/debug/axon setup repair --migrate-env --json
```

Expected: output includes a backup path and counts; output does not include secret values.

- [ ] **Step 4: Capture after counts without values**

Run:

```bash
printf 'after_lines='
wc -l < ~/.axon/.env
printf 'after_keys='
awk -F= '/^[[:space:]]*[A-Za-z_][A-Za-z0-9_]*=/ { count++ } END { print count+0 }' ~/.axon/.env
```

Expected: `after_lines` is <=30 unless the final compose/auth decision explicitly leaves more lines and the final report explains why.

- [ ] **Step 5: Source/parse smoke**

Run:

```bash
zsh -fc 'set -a; source ~/.axon/.env; set +a; print -r -- "env source ok"'
./scripts/axon doctor
./target/debug/axon status --json
```

Expected: source exits 0. `doctor` and `status` either succeed or report unavailable external services without config parse failures.

- [ ] **Step 6: Conditional live smokes**

If Qdrant, TEI, Chrome, and Gemini auth are available, run:

```bash
./scripts/axon query "configuration boundary" --limit 1 --json
./scripts/axon ask "What is Axon?" --json
```

If a required external service is unavailable, record the skip reason in the final report.

- [ ] **Step 7: Final report**

Report:

```text
backup_path=<path>
before_lines=<n>
before_keys=<n>
after_lines=<n>
after_keys=<n>
kept_env=<n>
compose_env=<n>
moved_toml=<n>
deleted=<n>
hard_defaulted=<n>
compatibility_shim=<n>
preserved_unclassified_backup_only=<n>
mandatory_gates=passed
conditional_gates=<passed|skipped with reason>
```

Do not include secret values.

- [ ] **Step 8: Commit source changes and push**

Only commit repo source/docs/tests, not live `~/.axon` files.

```bash
git status --short
git add docs/config/env-migration-matrix.toml scripts/check-env-config-boundary.py src tests docker-compose.yaml .env.example config.example.toml docs
git commit -m "feat(config): migrate env tuning to TOML boundary"
git pull --rebase
bd dolt push
git push
git status --short --branch
```

Expected final branch status: up to date with origin, with no unintended source changes.

---

## Self-Review

Spec coverage:

- `.env` <=30 lines: Task 6 and Task 9.
- Live `~/.axon/.env` backup and migration: Task 4 and Task 9.
- Source-derived matrix: Task 1.
- Host/container/compose placement: Task 1 and Task 6.
- `AXON_ENV_FILE` handling: Task 4 and Task 9.
- `[services]` TOML staged migration: Task 3 and Task 5.
- Central compatibility registry: Task 2 and Task 4.
- Setup/local and remote deploy not recreating sprawl: Task 4 and Task 5.
- Docs redaction and split-boundary rewrite: Task 8.
- Mandatory and conditional verification: Task 9.

Placeholder scan:

- The plan avoids open-ended implementation placeholders. Each task gives concrete files, commands, expected results, and code snippets for new interfaces.

Type consistency:

- `EnvClassification`, `RuntimePlacement`, `LegacyBehavior`, and `EnvKeySpec` are introduced in Task 2 and referenced by later tasks.
- `write_remote_runtime_env` is introduced in Task 5 before deploy uses it.
- `migrate_env_file` is introduced in Task 4 before verification uses `--migrate-env`.

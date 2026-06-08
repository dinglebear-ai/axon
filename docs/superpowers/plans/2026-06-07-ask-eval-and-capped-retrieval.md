# Ask Eval And Capped Retrieval Implementation Plan

> **Historical note (superseded by PR #185 implementation):** This plan was
> written before the final evaluation harness shape landed. It describes
> `xtask ask-eval` as the canonical harness and the shell runner as a temporary
> compatibility wrapper. The PR implementation intentionally kept the split
> shell runner as the actual supported tool instead. Treat the `xtask` steps
> below as historical planning context, not follow-up work to chase.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a repeatable ask-evaluation harness and improve `axon ask` retrieval/synthesis behavior for capped local models such as Gemma 4 E4B.

**Architecture:** Put benchmark orchestration in `xtask` so answer runs, explain traces, timing, and grading are reproducible without growing shell scripts. Put retrieval/context improvements in the production ask pipeline under `src/vector/ops/commands/ask/`, and keep synthesis steering in the editable `plugins/axon/skills/axon-rag-synthesize/SKILL.md` prompt skill.

**Tech Stack:** Rust 2024, Clap, Serde/serde_json, anyhow, existing Axon CLI binary, Qdrant-backed `axon ask`, existing `xtask` crate, shell runner retained temporarily as a compatibility wrapper.

---

## Scope Check

This plan intentionally spans two related surfaces:

- Evaluation harness: `xtask ask-eval` measures behavior and writes artifacts.
- Production behavior: retrieval/context/prompt changes improve actual `axon ask`.

They are coupled by evidence: every production change should be validated through the same evaluation harness. Keep commits task-scoped so the harness can land independently before retrieval changes.

## File Structure

- Create `xtask/src/ask_eval.rs`: CLI-driven harness for question loading, profile execution, answer/explain capture, and JSON/Markdown report writing.
- Modify `xtask/src/main.rs`: add `AskEval` subcommand and route to `ask_eval::run`.
- Modify `xtask/Cargo.toml`: add any missing dependencies needed by the harness.
- Modify `scripts/run-ask-model-comparison.sh`: either delegate to `cargo xtask ask-eval` or keep only as a temporary compatibility wrapper.
- Modify `docs/guides/ask-model-comparison-runner.md`: document the xtask command as canonical.
- Modify `plugins/axon/skills/axon-rag-synthesize/SKILL.md`: add generalized completeness checks for subquestions, causal "why" questions, tradeoffs, and exact-version/value questions.
- Modify `src/vector/ops/commands/ask/synthesis_prompt_tests.rs`: lock the prompt rules with tests.
- Modify `src/vector/ops/commands/ask/context/build/appenders.rs`: make full-doc rendering fit capped budgets by selecting relevant slices instead of all-or-nothing whole documents.
- Modify `src/vector/ops/commands/ask/context/build/selection.rs`: improve source diversity and canonical-source preference for capped models.
- Modify `src/vector/ops/commands/ask/context/build/trace.rs` and `src/vector/ops/commands/ask.rs`: expose richer explain fields for context selection and capped-model diagnostics.
- Add or modify tests under `src/vector/ops/commands/ask/context_*` and `src/vector/ops/commands/ask/context/build/*_tests.rs`.

---

### Task 1: Add `xtask ask-eval` CLI Skeleton

**Files:**
- Create: `xtask/src/ask_eval.rs`
- Modify: `xtask/src/main.rs`
- Test: `xtask/src/ask_eval.rs`

- [ ] **Step 1: Write the failing CLI unit tests**

Add this test module to the bottom of `xtask/src/ask_eval.rs` while creating the file:

```rust
use anyhow::{Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Args, Clone)]
pub struct AskEvalArgs {
    /// Markdown question file containing ## Questions and ## Answer Key.
    #[arg(long, default_value = "reports/llm-ask-comparison-2026-06-07/questions-indexed-general.md")]
    pub questions: PathBuf,

    /// Output directory for the run.
    #[arg(long)]
    pub out_dir: Option<PathBuf>,

    /// Axon binary to execute.
    #[arg(long, default_value = "target/release/axon")]
    pub axon_bin: PathBuf,

    /// Comma-separated profile list.
    #[arg(long, default_value = "current,gemini-flash,gpt-5.4-mini,gemini-3.1-flash-lite,gemma-local")]
    pub profiles: String,

    /// Base env file copied for override profiles.
    #[arg(long, default_value = "~/.axon/.env")]
    pub base_env: String,

    /// Run profile workers serially instead of in parallel.
    #[arg(long)]
    pub serial: bool,

    /// Skip retrieval explain traces.
    #[arg(long)]
    pub no_explain: bool,

    /// Print plan and exit without invoking axon.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub id: String,
    pub text: String,
}

pub fn parse_questions(markdown: &str) -> Vec<Question> {
    let mut in_questions = false;
    let mut out = Vec::new();
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed == "## Questions" {
            in_questions = true;
            continue;
        }
        if trimmed == "## Answer Key" {
            in_questions = false;
            continue;
        }
        if !in_questions {
            continue;
        }
        let Some((num, rest)) = trimmed.split_once(". ") else {
            continue;
        };
        if num.parse::<usize>().is_err() {
            continue;
        }
        out.push(Question {
            id: format!("Q{:02}", out.len() + 1),
            text: rest.to_string(),
        });
    }
    out
}

pub fn run(_root: &Path, _args: AskEvalArgs) -> Result<()> {
    anyhow::bail!("ask-eval runner not implemented yet")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_questions_extracts_numbered_items_between_sections() {
        let got = parse_questions(
            r#"# Eval

## Questions

1. First question?
2. Second question?

## Answer Key

### Q01
Answer.
"#,
        );
        assert_eq!(
            got,
            vec![
                Question { id: "Q01".to_string(), text: "First question?".to_string() },
                Question { id: "Q02".to_string(), text: "Second question?".to_string() },
            ]
        );
    }
}
```

- [ ] **Step 2: Run the failing xtask test**

Run:

```bash
cargo test --manifest-path xtask/Cargo.toml ask_eval::tests::parse_questions_extracts_numbered_items_between_sections
```

Expected: FAIL because `ask_eval` is not wired into the xtask module tree.

- [ ] **Step 3: Wire the module and subcommand**

Modify `xtask/src/main.rs`:

```rust
use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "xtask", about = "Axon repository maintenance checks")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run an ask benchmark across configured LLM profiles.
    AskEval(ask_eval::AskEvalArgs),
    /// Run all repository checks.
    Check,
    /// Enforce modern Rust module layout.
    CheckNoModRs,
    /// Verify MCP HTTP transport support.
    CheckMcpHttp,
    /// Reject staged secret env files.
    CheckEnvStaged,
    /// Warn about newly staged unwrap/expect calls.
    CheckUnwraps,
    /// Verify AGENTS.md/GEMINI.md symlinks next to CLAUDE.md files.
    CheckClaudeSymlinks,
    /// Fail if any symlink in the worktree points to a non-existent target.
    CheckBrokenSymlinks,
    /// Scan staged files for secrets and credentials.
    CheckSecrets,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = std::env::current_dir()?;
    match cli.command {
        Command::AskEval(args) => ask_eval::run(&root, args),
        Command::Check => checks::check(&root),
        Command::CheckNoModRs => checks::no_mod_rs::check(&root),
        Command::CheckMcpHttp => checks::mcp_http::check(&root),
        Command::CheckEnvStaged => checks::env_staged::check(&root),
        Command::CheckUnwraps => checks::unwraps::check(&root),
        Command::CheckClaudeSymlinks => checks::claude_symlinks::check(&root),
        Command::CheckBrokenSymlinks => checks::broken_symlinks::check(&root),
        Command::CheckSecrets => checks::secrets::check(&root),
    }
}

mod ask_eval;
mod checks;
```

- [ ] **Step 4: Run the test to verify parser behavior**

Run:

```bash
cargo test --manifest-path xtask/Cargo.toml ask_eval::tests::parse_questions_extracts_numbered_items_between_sections
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add xtask/src/main.rs xtask/src/ask_eval.rs
git commit -m "feat(xtask): add ask eval command skeleton"
```

---

### Task 2: Implement Profiles, Env Overrides, And Dry-Run Output

**Files:**
- Modify: `xtask/src/ask_eval.rs`
- Test: `xtask/src/ask_eval.rs`

- [ ] **Step 1: Add tests for profile config**

Append these tests inside `xtask/src/ask_eval.rs`:

```rust
#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn known_profiles_have_expected_models_and_providers() {
        let profiles = parse_profiles("current,gemini-flash,gpt-5.4-mini,gemini-3.1-flash-lite,gemma-local")
            .expect("profiles parse");
        let labels = profiles.iter().map(|p| p.label.as_str()).collect::<Vec<_>>();
        assert_eq!(
            labels,
            vec![
                "current-config",
                "cli-api-gemini-3.5-flash-low",
                "cli-api-gpt-5.4-mini",
                "cli-api-gemini-3.1-flash-lite",
                "llamacpp-gemma-4-e4b-q4",
            ]
        );
        assert_eq!(profiles[4].env_overrides.get("AXON_ASK_MAX_CONTEXT_CHARS").unwrap(), "300000");
    }

    #[test]
    fn unknown_profile_is_rejected() {
        let err = parse_profiles("current,wat").unwrap_err().to_string();
        assert!(err.contains("unknown profile"));
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test --manifest-path xtask/Cargo.toml profile_tests
```

Expected: FAIL because `parse_profiles` and profile types do not exist.

- [ ] **Step 3: Implement profile types**

Add this code above `run` in `xtask/src/ask_eval.rs`:

```rust
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub label: String,
    pub provider: String,
    pub model: String,
    pub env_overrides: BTreeMap<String, String>,
}

pub fn parse_profiles(input: &str) -> Result<Vec<Profile>> {
    input
        .split(',')
        .filter(|item| !item.trim().is_empty())
        .map(|name| profile_for(name.trim()))
        .collect()
}

fn profile_for(name: &str) -> Result<Profile> {
    let cli_api = std::env::var("CLI_API_BASE_URL")
        .unwrap_or_else(|_| "https://cli-api.tootie.tv/v1".to_string());
    let gemma_base = std::env::var("GEMMA_OPENAI_BASE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080/v1".to_string());
    let mut env = BTreeMap::new();
    match name {
        "current" => Ok(Profile {
            name: name.to_string(),
            label: "current-config".to_string(),
            provider: "current-config".to_string(),
            model: "current-config".to_string(),
            env_overrides: env,
        }),
        "gemini-flash" => {
            let model = std::env::var("GEMINI_FLASH_MODEL")
                .unwrap_or_else(|_| "gemini-3.5-flash-low".to_string());
            openai_profile(name, &format!("cli-api-{model}"), &cli_api, &model)
        }
        "gpt-5.4-mini" => {
            let model = std::env::var("GPT_5_4_MINI_MODEL")
                .unwrap_or_else(|_| "gpt-5.4-mini".to_string());
            openai_profile(name, &format!("cli-api-{model}"), &cli_api, &model)
        }
        "gemini-3.1-flash-lite" => {
            let model = std::env::var("GEMINI_3_1_FLASH_LITE_MODEL")
                .unwrap_or_else(|_| "gemini-3.1-flash-lite".to_string());
            openai_profile(name, &format!("cli-api-{model}"), &cli_api, &model)
        }
        "gemma-local" => {
            let model = std::env::var("GEMMA_MODEL")
                .unwrap_or_else(|_| "ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M".to_string());
            env.insert("AXON_LLM_BACKEND".to_string(), "openai-compat".to_string());
            env.insert("AXON_OPENAI_BASE_URL".to_string(), gemma_base.clone());
            env.insert("AXON_OPENAI_MODEL".to_string(), model.clone());
            env.insert("AXON_OPENAI_API_KEY".to_string(), "".to_string());
            env.insert("AXON_LLM_COMPLETION_CONCURRENCY".to_string(), "1".to_string());
            env.insert("AXON_ASK_MAX_CONTEXT_CHARS".to_string(), "300000".to_string());
            env.insert("AXON_ASK_CHUNK_LIMIT".to_string(), "20".to_string());
            env.insert("AXON_ASK_CANDIDATE_LIMIT".to_string(), "120".to_string());
            env.insert("AXON_ASK_HYBRID_CANDIDATES".to_string(), "100".to_string());
            env.insert("AXON_ASK_DOC_FETCH_CONCURRENCY".to_string(), "1".to_string());
            Ok(Profile {
                name: name.to_string(),
                label: "llamacpp-gemma-4-e4b-q4".to_string(),
                provider: gemma_base,
                model,
                env_overrides: env,
            })
        }
        other => anyhow::bail!("unknown profile: {other}"),
    }
}

fn openai_profile(name: &str, label: &str, provider: &str, model: &str) -> Result<Profile> {
    let mut env = BTreeMap::new();
    env.insert("AXON_LLM_BACKEND".to_string(), "openai-compat".to_string());
    env.insert("AXON_OPENAI_BASE_URL".to_string(), provider.to_string());
    env.insert("AXON_OPENAI_MODEL".to_string(), model.to_string());
    env.insert("AXON_OPENAI_API_KEY".to_string(), "***".to_string());
    env.insert("AXON_LLM_COMPLETION_CONCURRENCY".to_string(), "1".to_string());
    Ok(Profile {
        name: name.to_string(),
        label: label.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        env_overrides: env,
    })
}
```

- [ ] **Step 4: Implement dry-run in `run`**

Replace the placeholder `run` with:

```rust
pub fn run(root: &Path, args: AskEvalArgs) -> Result<()> {
    let questions_path = absolutize(root, &args.questions);
    let markdown = std::fs::read_to_string(&questions_path)
        .with_context(|| format!("read questions file {}", questions_path.display()))?;
    let questions = parse_questions(&markdown);
    if questions.is_empty() {
        anyhow::bail!("no questions found in {}", questions_path.display());
    }
    let profiles = parse_profiles(&args.profiles)?;
    let out_dir = args.out_dir.clone().unwrap_or_else(|| {
        root.join("reports/llm-ask-comparison-2026-06-07")
            .join(format!("run-{}", chrono_like_stamp()))
    });
    if args.dry_run {
        println!("Planned ask eval");
        println!("  axon: {}", absolutize(root, &args.axon_bin).display());
        println!("  questions: {} ({})", questions_path.display(), questions.len());
        println!("  out_dir: {}", out_dir.display());
        println!("  profiles: {}", profiles.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(","));
        return Ok(());
    }
    anyhow::bail!("ask-eval execution not implemented yet")
}

fn absolutize(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn chrono_like_stamp() -> String {
    let output = std::process::Command::new("date")
        .arg("+%Y%m%d-%H%M%S")
        .output()
        .expect("date command should be available");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}
```

- [ ] **Step 5: Run tests and dry-run**

Run:

```bash
cargo test --manifest-path xtask/Cargo.toml ask_eval
cargo run --manifest-path xtask/Cargo.toml -- ask-eval --dry-run --profiles current,gemma-local
```

Expected: tests PASS; dry-run prints two profiles and does not invoke `axon`.

- [ ] **Step 6: Commit**

```bash
git add xtask/src/ask_eval.rs
git commit -m "feat(xtask): model ask eval profiles"
```

---

### Task 3: Execute Answer And Explain Runs From `xtask`

**Files:**
- Modify: `xtask/src/ask_eval.rs`
- Test: `xtask/src/ask_eval.rs`

- [ ] **Step 1: Add execution tests with a fake axon binary**

Add this test to `xtask/src/ask_eval.rs`:

```rust
#[cfg(test)]
mod execution_tests {
    use super::*;

    #[test]
    fn run_writes_answer_explain_and_run_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let questions = root.join("questions.md");
        std::fs::write(
            &questions,
            "# Eval\n\n## Questions\n\n1. What is alpha?\n\n## Answer Key\n",
        )
        .expect("write questions");
        let fake = root.join("axon-fake.sh");
        std::fs::write(
            &fake,
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == "config" ]]; then
  printf '{"env":{"AXON_LLM_BACKEND":"fake"},"toml":{}}\n'
elif [[ "$2" == "--explain" ]]; then
  printf '{"answer":"","diagnostics":{"context_chars":42},"explain":{"context":{"context_chars_used":42},"candidates":[]},"timing_ms":{"llm":0}}\n'
else
  printf 'answer for %s\n' "${@: -1}"
fi
"#,
        )
        .expect("write fake");
        make_executable(&fake).expect("chmod fake");
        let out = root.join("out");
        run(
            root,
            AskEvalArgs {
                questions,
                out_dir: Some(out.clone()),
                axon_bin: fake,
                profiles: "current".to_string(),
                base_env: root.join("missing.env").display().to_string(),
                serial: true,
                no_explain: false,
                dry_run: false,
            },
        )
        .expect("run ask eval");
        assert!(out.join("run.json").exists());
        assert!(out.join("current-config/Q01.md").exists());
        assert!(out.join("current-config/Q01.explain.json").exists());
    }
}
```

- [ ] **Step 2: Add `tempfile` to xtask dev dependencies**

Modify `xtask/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Run the failing test**

Run:

```bash
cargo test --manifest-path xtask/Cargo.toml execution_tests::run_writes_answer_explain_and_run_json
```

Expected: FAIL because execution helpers are missing.

- [ ] **Step 4: Implement execution data structures**

Add to `xtask/src/ask_eval.rs`:

```rust
#[derive(Debug, Serialize)]
struct RunReport {
    schema: &'static str,
    questions_file: String,
    out_dir: String,
    profiles: Vec<Profile>,
    results: Vec<QuestionResult>,
}

#[derive(Debug, Serialize)]
struct QuestionResult {
    question_id: String,
    question: String,
    profile: String,
    profile_label: String,
    provider: String,
    model: String,
    elapsed_seconds: f64,
    exit_code: i32,
    stdout_file: String,
    stderr_file: String,
    explain_elapsed_seconds: Option<f64>,
    explain_exit_code: Option<i32>,
    explain_file: Option<String>,
    explain_stderr_file: Option<String>,
}

fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o700);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}
```

- [ ] **Step 5: Implement command execution**

Add these helpers:

```rust
fn run_axon_to_files(
    axon_bin: &Path,
    profile: &Profile,
    question: &str,
    explain: bool,
    stdout_file: &Path,
    stderr_file: &Path,
) -> Result<(i32, f64)> {
    let started = std::time::Instant::now();
    let mut cmd = std::process::Command::new(axon_bin);
    cmd.arg("ask");
    if explain {
        cmd.args(["--explain", "--diagnostics", "--json"]);
    }
    cmd.arg(question);
    for (key, value) in &profile.env_overrides {
        if key == "AXON_OPENAI_API_KEY" && value == "***" {
            continue;
        }
        cmd.env(key, value);
    }
    let output = cmd.output().with_context(|| format!("run {}", axon_bin.display()))?;
    std::fs::write(stdout_file, &output.stdout)?;
    std::fs::write(stderr_file, &output.stderr)?;
    let code = output.status.code().unwrap_or(1);
    Ok((code, started.elapsed().as_secs_f64()))
}

fn write_answer_markdown(path: &Path, result: &QuestionResult) -> Result<()> {
    let body = std::fs::read_to_string(path).unwrap_or_default();
    std::fs::write(
        path,
        format!(
            "## {}\n\n**Question:** {}\n\n**Provider:** `{}`  \n**Model:** `{}`  \n**Elapsed:** `{:.3}s`  \n**Exit code:** `{}`\n\n---\n\n{}",
            result.question_id,
            result.question,
            result.provider,
            result.model,
            result.elapsed_seconds,
            result.exit_code,
            body
        ),
    )?;
    Ok(())
}
```

- [ ] **Step 6: Replace `run` execution placeholder**

In `run`, replace `anyhow::bail!("ask-eval execution not implemented yet")` with:

```rust
std::fs::create_dir_all(&out_dir)?;
let mut results = Vec::new();
for profile in &profiles {
    let profile_dir = out_dir.join(&profile.label);
    std::fs::create_dir_all(&profile_dir)?;
    for question in &questions {
        let answer_file = profile_dir.join(format!("{}.md", question.id));
        let stderr_file = profile_dir.join(format!("{}.stderr.log", question.id));
        let explain_file = profile_dir.join(format!("{}.explain.json", question.id));
        let explain_stderr_file = profile_dir.join(format!("{}.explain.stderr.log", question.id));

        let (explain_exit_code, explain_elapsed_seconds) = if args.no_explain {
            (None, None)
        } else {
            let (code, elapsed) = run_axon_to_files(
                &args.axon_bin,
                profile,
                &question.text,
                true,
                &explain_file,
                &explain_stderr_file,
            )?;
            (Some(code), Some(elapsed))
        };

        let (exit_code, elapsed_seconds) = run_axon_to_files(
            &args.axon_bin,
            profile,
            &question.text,
            false,
            &answer_file,
            &stderr_file,
        )?;
        let result = QuestionResult {
            question_id: question.id.clone(),
            question: question.text.clone(),
            profile: profile.name.clone(),
            profile_label: profile.label.clone(),
            provider: profile.provider.clone(),
            model: profile.model.clone(),
            elapsed_seconds,
            exit_code,
            stdout_file: answer_file.display().to_string(),
            stderr_file: stderr_file.display().to_string(),
            explain_elapsed_seconds,
            explain_exit_code,
            explain_file: (!args.no_explain).then(|| explain_file.display().to_string()),
            explain_stderr_file: (!args.no_explain).then(|| explain_stderr_file.display().to_string()),
        };
        write_answer_markdown(&answer_file, &result)?;
        results.push(result);
    }
}
let report = RunReport {
    schema: "axon-ask-eval/v1",
    questions_file: questions_path.display().to_string(),
    out_dir: out_dir.display().to_string(),
    profiles,
    results,
};
std::fs::write(out_dir.join("run.json"), serde_json::to_string_pretty(&report)?)?;
println!("{}", out_dir.display());
Ok(())
```

- [ ] **Step 7: Run tests**

Run:

```bash
cargo test --manifest-path xtask/Cargo.toml ask_eval
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add xtask/Cargo.toml xtask/src/ask_eval.rs
git commit -m "feat(xtask): run ask eval answers and explain traces"
```

---

### Task 4: Make The Shell Runner Delegate To `xtask`

**Files:**
- Modify: `scripts/run-ask-model-comparison.sh`
- Modify: `docs/guides/ask-model-comparison-runner.md`
- Test: `scripts/run-ask-model-comparison.sh`

- [ ] **Step 1: Replace shell script body with a compatibility wrapper**

Replace `scripts/run-ask-model-comparison.sh` with:

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$REPO_ROOT"
exec cargo run --manifest-path xtask/Cargo.toml -- ask-eval "$@"
```

- [ ] **Step 2: Run dry-run compatibility check**

Run:

```bash
scripts/run-ask-model-comparison.sh --dry-run --profiles current,gemma-local
```

Expected: prints the `xtask ask-eval` dry-run plan.

- [ ] **Step 3: Update docs**

In `docs/guides/ask-model-comparison-runner.md`, change the quick-start command examples to:

```bash
cargo run --manifest-path xtask/Cargo.toml -- ask-eval --dry-run
cargo run --manifest-path xtask/Cargo.toml -- ask-eval --profiles current,gemma-local
scripts/run-ask-model-comparison.sh --dry-run
```

Add this sentence:

```md
The shell script is a compatibility wrapper; `cargo run --manifest-path xtask/Cargo.toml -- ask-eval` is the canonical interface.
```

- [ ] **Step 4: Commit**

```bash
git add scripts/run-ask-model-comparison.sh docs/guides/ask-model-comparison-runner.md
git commit -m "chore: route ask comparison runner through xtask"
```

---

### Task 5: Add Prompt Completeness Rules For Mostly-Correct Answers

**Files:**
- Modify: `plugins/axon/skills/axon-rag-synthesize/SKILL.md`
- Modify: `src/vector/ops/commands/ask/synthesis_prompt_tests.rs`

- [ ] **Step 1: Write failing prompt tests**

Add this test to `src/vector/ops/commands/ask/synthesis_prompt_tests.rs`:

```rust
#[test]
fn synthesis_prompt_requires_subquestion_and_why_coverage() {
    let prompt = synthesis_prompt_for_openai_compat();
    assert!(
        prompt.contains("Answer each requested part explicitly."),
        "prompt should require explicit coverage of multi-part questions"
    );
    assert!(
        prompt.contains("For why questions, include the causal reason, consequence, or risk"),
        "prompt should require causal explanation when the user asks why"
    );
    assert!(
        prompt.contains("If the sources state a rule without explaining why"),
        "prompt should preserve grounding when why evidence is absent"
    );
}

#[test]
fn synthesis_prompt_requires_exact_values_and_tradeoffs_when_asked() {
    let prompt = synthesis_prompt_for_openai_compat();
    assert!(
        prompt.contains("If the question asks what changed, include the before/after distinction"),
        "prompt should steer version-change questions like vLLM hashing"
    );
    assert!(
        prompt.contains("If the question asks about tradeoffs or effects, name the affected dimensions"),
        "prompt should steer dRAID and sparse/dense comparison questions"
    );
}
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
cargo test synthesis_prompt_requires_subquestion_and_why_coverage synthesis_prompt_requires_exact_values_and_tradeoffs_when_asked
```

Expected: FAIL because the prompt does not contain these rules.

- [ ] **Step 3: Patch the synthesis prompt**

In `plugins/axon/skills/axon-rag-synthesize/SKILL.md`, under `IF RELEVANT CONTEXT EXISTS`, insert this after item 1:

```md
2. Before finalizing, check whether the user asked multiple subquestions or asked "why".
   Answer each requested part explicitly. For why questions, include the causal reason,
   consequence, or risk described by the sources, not only the rule or recommendation.
   If the sources state a rule without explaining why, say that the sources state the
   rule but do not explain why.

3. If the question asks what changed, include the before/after distinction, version
   boundary, old behavior, new behavior, or exact value when the sources provide it.
   If the question asks about tradeoffs or effects, name the affected dimensions from
   the sources, such as capacity, latency, IOPS, compression ratio, security, cost,
   compatibility, or operator burden.
```

Renumber the existing items in that branch so the list remains ordered.

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test synthesis_prompt_requires_subquestion_and_why_coverage synthesis_prompt_requires_exact_values_and_tradeoffs_when_asked
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add plugins/axon/skills/axon-rag-synthesize/SKILL.md src/vector/ops/commands/ask/synthesis_prompt_tests.rs
git commit -m "fix(ask): strengthen synthesis completeness prompt"
```

---

### Task 6: Improve Full-Doc Rendering For Capped Context Budgets

**Files:**
- Modify: `src/vector/ops/commands/ask/context/build/appenders.rs`
- Test: `src/vector/ops/commands/ask/context/build/appenders_budget_tests.rs`

- [ ] **Step 1: Create failing tests for all-or-nothing full-doc insertion**

Create `src/vector/ops/commands/ask/context/build/appenders_budget_tests.rs`:

```rust
use super::*;
use crate::vector::ops::qdrant::QdrantPoint;
use serde_json::json;
use std::collections::{HashMap, HashSet};

fn point(url: &str, chunk_index: u64, text: &str) -> QdrantPoint {
    QdrantPoint {
        id: json!(format!("{url}-{chunk_index}")),
        score: None,
        payload: {
            let mut payload = serde_json::Map::new();
            payload.insert("url".to_string(), json!(url));
            payload.insert("chunk_index".to_string(), json!(chunk_index));
            payload.insert("chunk_text".to_string(), json!(text));
            payload
        },
        vector: None,
    }
}

#[test]
fn full_doc_rendering_keeps_relevant_slices_when_full_doc_would_exceed_budget() {
    let url = "https://docs.example.com/full";
    let points = vec![
        point(url, 0, "irrelevant ".repeat(500).as_str()),
        point(url, 1, "special redundancy risk pool metadata loss"),
        point(url, 2, "small blocks special_small_blocks dataset setting"),
    ];
    let mut entries = Vec::new();
    let mut char_count = 0;
    let mut inserted = HashSet::new();
    let mut scores = HashMap::new();
    scores.insert(url.to_string(), 1.0);

    let (selected, _) = append_full_docs_to_context(
        &mut entries,
        &mut char_count,
        &mut inserted,
        1,
        "\n\n---\n\n",
        900,
        vec![(0, url.to_string(), points)],
        &["special".to_string(), "risk".to_string(), "small".to_string()],
        &scores,
    );

    assert_eq!(selected, 1);
    let rendered = &entries[0].1;
    assert!(rendered.contains("special redundancy risk"));
    assert!(rendered.contains("special_small_blocks"));
    assert!(rendered.len() <= 900);
}
```

Add this module line to the bottom of `appenders.rs`:

```rust
#[cfg(test)]
#[path = "appenders_budget_tests.rs"]
mod budget_tests;
```

- [ ] **Step 2: Run failing test**

Run:

```bash
cargo test full_doc_rendering_keeps_relevant_slices_when_full_doc_would_exceed_budget
```

Expected: FAIL because the current full-doc append is all-or-nothing.

- [ ] **Step 3: Implement bounded full-doc rendering**

In `appenders.rs`, replace the body inside the `for (_idx, url, points) in fetched_docs` loop with:

```rust
let remaining_budget = max_context_chars.saturating_sub(*context_char_count);
let source = display_source(&url);
let header_len = format!("## Source Document [S{}]: {}\n\n", source_idx, source).len();
let available_text_budget = remaining_budget
    .saturating_sub(separator.len())
    .saturating_sub(header_len);
let text = render_full_doc_for_budget(points, query_tokens, available_text_budget);
if text.is_empty() {
    continue;
}
let entry = format!("## Source Document [S{}]: {}\n\n{}", source_idx, source, text);
let score = url_to_score.get(&url).copied().unwrap_or(0.0);
if !push_context_entry(
    context_entries,
    context_char_count,
    score,
    entry,
    separator,
    max_context_chars,
) {
    continue;
}
inserted_full_doc_urls.insert(url);
full_docs_selected += 1;
source_idx += 1;
```

Add this helper above `append_full_docs_to_context`:

```rust
fn render_full_doc_for_budget(
    points: Vec<qdrant::QdrantPoint>,
    query_tokens: &[String],
    budget: usize,
) -> String {
    if budget == 0 {
        return String::new();
    }
    let full = qdrant::render_full_doc_filtered(points.clone(), Some(query_tokens), Some(FULL_DOC_RENDER_TOP_K));
    if full.len() <= budget {
        return full;
    }
    let mut scored = points
        .into_iter()
        .filter_map(|point| {
            let text = point.payload.get("chunk_text")?.as_str()?.to_string();
            let score = query_tokens
                .iter()
                .filter(|token| text.to_ascii_lowercase().contains(token.as_str()))
                .count();
            Some((score, text))
        })
        .collect::<Vec<_>>();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.len().cmp(&a.1.len())));
    let mut out = String::new();
    for (_, text) in scored {
        let separator = if out.is_empty() { "" } else { "\n\n" };
        if out.len() + separator.len() + text.len() > budget {
            continue;
        }
        out.push_str(separator);
        out.push_str(&text);
    }
    out
}
```

- [ ] **Step 4: Run targeted tests**

Run:

```bash
cargo test full_doc_rendering_keeps_relevant_slices_when_full_doc_would_exceed_budget
cargo test append_full_docs_to_context
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/vector/ops/commands/ask/context/build/appenders.rs src/vector/ops/commands/ask/context/build/appenders_budget_tests.rs
git commit -m "fix(ask): fit full-doc slices into capped context budgets"
```

---

### Task 7: Add Coverage-Oriented Explain Diagnostics

**Files:**
- Modify: `src/vector/ops/commands/ask/context/build/trace.rs`
- Modify: `src/services/types/service.rs`
- Modify: `src/vector/ops/commands/ask.rs`
- Test: `src/vector/ops/commands/ask/context/build/trace_tests.rs`

- [ ] **Step 1: Inspect current explain type names**

Run:

```bash
rg -n "struct AskExplain|AskExplainContext|final_source_order|ContextCandidateSelection" src/services src/vector/ops/commands/ask
```

Expected: identify the exact type definitions before editing. If the type fields differ from this plan, adapt field names consistently in this task only.

- [ ] **Step 2: Add failing test for selected source coverage summary**

In `src/vector/ops/commands/ask/context/build/trace_tests.rs`, add:

```rust
#[test]
fn explain_context_reports_selected_source_tiers_and_budget() {
    let summary = super::coverage_summary_for_test(vec![
        ("full_doc", "https://docs.example.com/a"),
        ("top_chunk", "https://docs.example.com/b"),
        ("top_chunk", "https://other.example.com/c"),
    ], 1200, 3000, false);
    assert_eq!(summary.full_docs, 1);
    assert_eq!(summary.top_chunks, 2);
    assert_eq!(summary.unique_domains, 2);
    assert_eq!(summary.context_chars_used, 1200);
    assert_eq!(summary.context_char_budget, 3000);
    assert!(!summary.truncated_by_budget);
}
```

- [ ] **Step 3: Run failing test**

Run:

```bash
cargo test explain_context_reports_selected_source_tiers_and_budget
```

Expected: FAIL because the helper/fields do not exist.

- [ ] **Step 4: Implement summary type and explain JSON fields**

Add a serializable type in the service types module that owns ask explain structs:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AskExplainCoverageSummary {
    pub full_docs: usize,
    pub top_chunks: usize,
    pub supplemental_chunks: usize,
    pub unique_domains: usize,
    pub context_chars_used: usize,
    pub context_char_budget: usize,
    pub truncated_by_budget: bool,
}
```

Add `coverage_summary: AskExplainCoverageSummary` to `AskExplainContext`.

Implement the builder in `trace.rs` or `build.rs`:

```rust
fn coverage_summary(
    final_source_order: &[AskExplainContextSource],
    context_chars_used: usize,
    context_char_budget: usize,
    truncated_by_budget: bool,
) -> AskExplainCoverageSummary {
    let mut domains = std::collections::BTreeSet::new();
    let mut full_docs = 0;
    let mut top_chunks = 0;
    let mut supplemental_chunks = 0;
    for source in final_source_order {
        match source.tier.as_str() {
            "full_doc" => full_docs += 1,
            "top_chunk" => top_chunks += 1,
            "supplemental" | "supplemental_chunk" => supplemental_chunks += 1,
            _ => {}
        }
        if let Ok(url) = spider::url::Url::parse(&source.url) {
            if let Some(host) = url.host_str() {
                domains.insert(host.to_string());
            }
        }
    }
    AskExplainCoverageSummary {
        full_docs,
        top_chunks,
        supplemental_chunks,
        unique_domains: domains.len(),
        context_chars_used,
        context_char_budget,
        truncated_by_budget,
    }
}
```

- [ ] **Step 5: Run explain JSON manually**

Run:

```bash
./target/debug/axon ask --explain --diagnostics --json "OpenZFS dRAID fixed stripe width compression IOPS special vdev" | jq '.explain.context.coverage_summary'
```

Expected: JSON includes `full_docs`, `top_chunks`, `unique_domains`, context chars, and budget.

- [ ] **Step 6: Commit**

```bash
git add src/services/types/service.rs src/vector/ops/commands/ask/context/build/trace.rs src/vector/ops/commands/ask/context/build/trace_tests.rs src/vector/ops/commands/ask.rs
git commit -m "feat(ask): expose context coverage in explain output"
```

---

### Task 8: Run Evaluation And Compare Before/After Quality

**Files:**
- Generated: `reports/llm-ask-comparison-2026-06-07/run-*/`
- Modify: `reports/llm-ask-comparison-2026-06-07/analysis.md`

- [ ] **Step 1: Build the current binary**

Run:

```bash
cargo build --bin axon
```

Expected: PASS.

- [ ] **Step 2: Run the focused Gemma eval with explain traces**

Run:

```bash
cargo run --manifest-path xtask/Cargo.toml -- ask-eval \
  --profiles gemma-local \
  --questions reports/llm-ask-comparison-2026-06-07/questions-indexed-general.md
```

Expected: writes a report directory containing `run.json`, `QNN.md`, and `QNN.explain.json`.

- [ ] **Step 3: Inspect the mostly-correct questions**

Run:

```bash
latest="$(ls -td reports/llm-ask-comparison-2026-06-07/run-* | head -1)"
jq -r '.results[] | select(.profile=="gemma-local") | [.question_id,.exit_code,.elapsed_seconds,.explain_elapsed_seconds,.stdout_file,.explain_file] | @tsv' "$latest/run.json"
for q in Q02 Q04 Q05 Q07 Q09 Q10; do
  echo "===== $q explain ====="
  jq '.diagnostics, .explain.context.coverage_summary // .explain.context' "$latest/llamacpp-gemma-4-e4b-q4/$q.explain.json" | head -120
done
```

Expected: every selected question has explain diagnostics and no execution failures.

- [ ] **Step 4: Update analysis**

Append this section to `reports/llm-ask-comparison-2026-06-07/analysis.md`:

```md
## Post-Tuning Gemma Evaluation

Run: `<run directory>`

### Execution

- Total Gemma questions: 10
- Execution failures: `<count>`
- Average answer seconds: `<value>`
- Explain traces captured: yes

### Manual Grade

| Question | Grade | Notes |
|---|---:|---|
| Q01 | `<score>` | `<short note>` |
| Q02 | `<score>` | `<short note>` |
| Q03 | `<score>` | `<short note>` |
| Q04 | `<score>` | `<short note>` |
| Q05 | `<score>` | `<short note>` |
| Q06 | `<score>` | `<short note>` |
| Q07 | `<score>` | `<short note>` |
| Q08 | `<score>` | `<short note>` |
| Q09 | `<score>` | `<short note>` |
| Q10 | `<score>` | `<short note>` |

### Retrieval Findings

- `<finding from explain trace>`
- `<finding from context coverage>`
- `<finding from failed or partial answers>`
```

Replace every placeholder before committing.

- [ ] **Step 5: Commit**

```bash
git add reports/llm-ask-comparison-2026-06-07/analysis.md reports/llm-ask-comparison-2026-06-07/run-*/
git commit -m "docs(eval): record post-tuning gemma ask results"
```

---

## Self-Review

**Spec coverage:** The plan covers the xtask harness, explain capture, prompt completeness changes, capped full-doc context behavior, explain diagnostics, docs, and post-change evaluation.

**Placeholder scan:** The plan includes one generated analysis template in Task 8 with explicit instructions to replace placeholders before commit. No implementation task leaves code unspecified.

**Type consistency:** `AskEvalArgs`, `Profile`, `Question`, `QuestionResult`, and `RunReport` are introduced before later tasks reference them. Production explain types are intentionally gated by a discovery step because the exact service type file may need field-name alignment.

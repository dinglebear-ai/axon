use crate::core::config::Config;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::ingest::progress::PhaseReporter;
use crate::vector::ops::input::chunk_markdown;
use crate::vector::ops::{PreparedDoc, embed_prepared_docs};
use anyhow::Result;
use octocrab::Octocrab;

mod files;
mod issues;
pub(super) mod meta;
mod wiki;

use meta::{GitHubPayloadParams, build_github_payload};

/// Number of concurrent sub-tasks in `run_github_subtasks` (files, metadata, issues, PRs, wiki).
const GITHUB_SUBTASK_COUNT: usize = 5;
/// Hard ceiling on total GitHub ingest time — prevents a hung sub-task from blocking the job forever.
const GITHUB_INGEST_TOTAL_TIMEOUT_SECS: u64 = 3600; // 1 hour

// ── Shared repo context passed to all sub-tasks ──────────────────────────────

/// Common fields extracted once from `repos().get()` and shared across all
/// concurrent sub-tasks (files, issues, PRs, wiki, metadata).
pub(crate) struct GitHubCommonFields {
    pub owner: String,
    pub name: String,
    pub repo_slug: String,
    pub default_branch: String,
    pub repo_description: Option<String>,
    pub pushed_at: Option<String>,
    pub is_private: Option<bool>,
    pub has_wiki: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubTarget {
    pub owner: String,
    pub repo: String,
    pub repo_slug: String,
}

// ── Pure helper functions (re-exported for tests and cli command) ──────────────

/// Returns true if a file path should be indexed when --include-source is set.
/// Excludes lock files, generated files, binaries, and non-code files.
pub fn is_indexable_source_path(path: &str) -> bool {
    // Reject build artifact and tool cache directories.
    // Each entry includes both the bare prefix ("target/") and the
    // slash-prefixed form ("/target/") so we can check with starts_with
    // and contains without any per-call format! allocations.
    static EXCLUDED_PREFIXES: &[(&str, &str)] = &[
        ("target/", "/target/"),
        ("node_modules/", "/node_modules/"),
        ("dist/", "/dist/"),
        ("build/", "/build/"),
        ("out/", "/out/"),
        ("coverage/", "/coverage/"),
        ("vendor/", "/vendor/"),
        (".gradle/", "/.gradle/"),
        (".terraform/", "/.terraform/"),
        (".next/", "/.next/"),
        (".nuxt/", "/.nuxt/"),
        ("venv/", "/venv/"),
        (".venv/", "/.venv/"),
        ("env/", "/env/"),
        ("__pycache__/", "/__pycache__/"),
        (".pytest_cache/", "/.pytest_cache/"),
        (".mypy_cache/", "/.mypy_cache/"),
    ];
    if EXCLUDED_PREFIXES
        .iter()
        .any(|(prefix, inner)| path.starts_with(prefix) || path.contains(inner))
    {
        return false;
    }

    // Reject lock files by name suffix
    if path.ends_with(".lock") || path.ends_with("-lock.json") || path.ends_with(".lock.json") {
        return false;
    }

    // Accept known source extensions
    let accepted = [
        // Systems languages
        ".rs", ".c", ".cpp", ".h", ".hpp", ".zig", // JVM / .NET
        ".java", ".kt", ".kts", ".cs", ".gradle", // Scripting
        ".py", ".rb", ".php", ".lua", ".sh", // Web / frontend
        ".ts", ".js", ".tsx", ".jsx", // Go / Swift
        ".go", ".swift", // BEAM (Elixir / Erlang)
        ".ex", ".exs", ".erl", // Data science
        ".r", ".R", ".ipynb", // Config / schema / IaC
        ".toml", ".yaml", ".yml", ".json", ".proto", ".sql", ".tf", ".nix",
        // Documentation (also caught by is_indexable_doc_path)
        ".md", ".adoc",
    ];
    accepted.iter().any(|ext| path.ends_with(ext))
}

/// Returns true if a file path should always be indexed (markdown/docs), regardless of --include-source.
pub fn is_indexable_doc_path(path: &str) -> bool {
    let accepted = [".md", ".mdx", ".rst", ".txt", ".adoc"];
    accepted.iter().any(|ext| path.ends_with(ext))
}

/// Parse an "owner/repo" string into (owner, repo) parts.
/// Accepts both "owner/repo" and "https://github.com/owner/repo" forms.
pub fn parse_github_repo(input: &str) -> Option<(String, String)> {
    parse_github_target(input).map(|target| (target.owner, target.repo))
}

/// Parse an "owner/repo" string into a normalized GitHub target.
/// Accepts both "owner/repo" and "https://github.com/owner/repo" forms.
pub fn parse_github_target(input: &str) -> Option<GitHubTarget> {
    let (slug, is_url) = match input.strip_prefix("https://github.com/") {
        Some(rest) => (rest.trim_end_matches('/'), true),
        None => (input, false),
    };

    let mut parts = slug.split('/');
    let owner = parts.next().filter(|s| !s.is_empty())?;
    let repo = parts.next().filter(|s| !s.is_empty())?;
    // URL form accepts extra path segments (e.g. pasted /tree/main); slug form does not.
    if !is_url && parts.next().is_some() {
        return None;
    }

    // Strip .git suffix commonly found in clone URLs
    let repo = repo.strip_suffix(".git").unwrap_or(repo);

    if repo.is_empty() {
        return None;
    }

    let owner = owner.to_string();
    let repo = repo.to_string();
    let repo_slug = format!("{owner}/{repo}");

    Some(GitHubTarget {
        owner,
        repo,
        repo_slug,
    })
}

// ── Octocrab helpers ───────────────────────────────────────────────────────────

const OCTOCRAB_REQUEST_TIMEOUT_SECS: u64 = 60;

/// Build an Octocrab instance — authenticated if a token is set, else default (unauthenticated).
/// Applies a 60s read/write timeout via the hyper-timeout connector to prevent pagination hangs.
fn build_octocrab(cfg: &Config) -> Result<Octocrab> {
    let timeout = Some(std::time::Duration::from_secs(
        OCTOCRAB_REQUEST_TIMEOUT_SECS,
    ));
    let builder = Octocrab::builder()
        .set_read_timeout(timeout)
        .set_write_timeout(timeout);
    let octo = if let Some(token) = cfg.github_token.as_deref() {
        builder.personal_token(token.to_string()).build()?
    } else {
        builder.build()?
    };
    Ok(octo)
}

/// Embed repo-level metadata (description, language, topics, license) as a single document.
async fn embed_repo_metadata(
    cfg: &Config,
    repo: &octocrab::models::Repository,
    common: &GitHubCommonFields,
    reporter: &PhaseReporter,
) -> Result<usize> {
    reporter.report_phase("embedding_metadata").await;
    let owner_name = &common.repo_slug;
    let mut parts: Vec<String> = Vec::new();

    if let Some(desc) = &repo.description
        && !desc.is_empty()
    {
        parts.push(format!("Description: {desc}"));
    }
    if let Some(lang) = &repo.language
        && let Some(s) = lang.as_str()
    {
        parts.push(format!("Language: {s}"));
    }
    if let Some(topics) = &repo.topics
        && !topics.is_empty()
    {
        parts.push(format!("Topics: {}", topics.join(", ")));
    }
    if let Some(license) = &repo.license {
        parts.push(format!("License: {}", license.name));
    }
    if let Some(stars) = repo.stargazers_count {
        parts.push(format!("Stars: {stars}"));
    }

    if parts.is_empty() {
        return Ok(0);
    }

    let content = format!("# {owner_name}\n\n{}", parts.join("\n"));
    let url = format!("https://github.com/{owner_name}");
    let language = repo.language.as_ref().and_then(|v| v.as_str());
    let extra = build_github_payload(&GitHubPayloadParams {
        repo: common.name.clone(),
        owner: common.owner.clone(),
        content_kind: "repo_metadata".into(),
        default_branch: Some(common.default_branch.clone()),
        repo_description: common.repo_description.clone(),
        pushed_at: common.pushed_at.clone(),
        is_private: common.is_private,
        stars: repo.stargazers_count,
        forks: repo.forks_count,
        open_issues: repo.open_issues_count,
        language: language.map(|s| s.to_string()),
        topics: repo.topics.clone(),
        is_fork: repo.fork,
        is_archived: repo.archived,
        ..Default::default()
    });
    let chunks = chunk_markdown(&content);
    if chunks.is_empty() {
        return Ok(0);
    }
    let domain = "github.com".to_string();
    let doc = PreparedDoc {
        url,
        domain,
        chunks,
        source_type: "github".to_string(),
        content_type: "text",
        title: Some(owner_name.to_string()),
        extra: Some(extra),
        extractor_name: None,
    };
    let summary = embed_prepared_docs(cfg, vec![doc], None)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(summary.chunks_embedded)
}

// ── Main entry point ───────────────────────────────────────────────────────────

fn tally_results(results: [(&str, Result<usize>); 5], repo: &str) -> (usize, usize, usize) {
    let mut total = 0usize;
    let mut issues_count = 0usize;
    let mut prs_count = 0usize;
    for (label, result) in results {
        match result {
            Ok(n) => {
                log_info(&format!(
                    "github task_done task={label} repo={repo} chunks={n}"
                ));
                if label == "issues" {
                    issues_count = n;
                } else if label == "prs" {
                    prs_count = n;
                }
                total += n;
            }
            Err(e) => log_warn(&format!(
                "command=ingest_github {label}_failed repo={repo} err={e}"
            )),
        }
    }
    (total, issues_count, prs_count)
}

/// Run all five GitHub sub-tasks concurrently and report per-task progress.
///
/// `tokio::join!` runs all branches on the same task (not spawned), so shared
/// borrows of `cfg`, `common`, `octo`, and `repo_info` work without Send issues.
/// Only `reporter` and `tasks_done` are cloned into per-branch local bindings.
async fn run_github_subtasks(
    cfg: &Config,
    common: &GitHubCommonFields,
    repo_info: &octocrab::models::Repository,
    octo: &Octocrab,
    include_source: bool,
    reporter: &PhaseReporter,
) -> [(&'static str, Result<usize>); 5] {
    let tasks_done = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let (files_result, metadata_result, issues_result, prs_result, wiki_result) = tokio::join!(
        async {
            let r = reporter.clone();
            let td = tasks_done.clone();
            let result =
                files::embed_files(cfg, common, include_source, cfg.github_token.as_deref(), &r)
                    .await;
            let done = td.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            r.report(serde_json::json!({"tasks_done": done})).await;
            log_info(&format!(
                "github task_complete task=files tasks_done={done}/{} repo={}",
                GITHUB_SUBTASK_COUNT, common.repo_slug
            ));
            result
        },
        async {
            let r = reporter.clone();
            let td = tasks_done.clone();
            let result = embed_repo_metadata(cfg, repo_info, common, &r).await;
            let done = td.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            r.report(serde_json::json!({"tasks_done": done})).await;
            log_info(&format!(
                "github task_complete task=metadata tasks_done={done}/{} repo={}",
                GITHUB_SUBTASK_COUNT, common.repo_slug
            ));
            result
        },
        async {
            let r = reporter.clone();
            let td = tasks_done.clone();
            let result = issues::ingest_issues(cfg, octo, common, cfg.github_max_issues, &r).await;
            let done = td.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            r.report(serde_json::json!({"tasks_done": done})).await;
            log_info(&format!(
                "github task_complete task=issues tasks_done={done}/{} repo={}",
                GITHUB_SUBTASK_COUNT, common.repo_slug
            ));
            result
        },
        async {
            let r = reporter.clone();
            let td = tasks_done.clone();
            let result =
                issues::ingest_pull_requests(cfg, octo, common, cfg.github_max_prs, &r).await;
            let done = td.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            r.report(serde_json::json!({"tasks_done": done})).await;
            log_info(&format!(
                "github task_complete task=prs tasks_done={done}/{} repo={}",
                GITHUB_SUBTASK_COUNT, common.repo_slug
            ));
            result
        },
        async {
            let r = reporter.clone();
            let td = tasks_done.clone();
            let result = if common.has_wiki {
                wiki::ingest_wiki(cfg, common, cfg.github_token.as_deref(), &r).await
            } else {
                Ok(0)
            };
            let done = td.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            r.report(serde_json::json!({"tasks_done": done})).await;
            log_info(&format!(
                "github task_complete task=wiki tasks_done={done}/{} repo={}",
                GITHUB_SUBTASK_COUNT, common.repo_slug
            ));
            result
        },
    );

    [
        ("files", files_result),
        ("metadata", metadata_result),
        ("issues", issues_result),
        ("prs", prs_result),
        ("wiki", wiki_result),
    ]
}

/// Ingest a GitHub repository: files, metadata, issues, PRs, and wiki.
///
/// Each sub-task is run concurrently via `tokio::join!`. Individual failures
/// are logged and counted as zero rather than aborting the whole run.
///
/// The `reporter` sends live progress updates as files are embedded and
/// sub-tasks complete. The worker uses this to persist progress to
/// `result_json` so `axon ingest list` and `axon status` show live data.
pub async fn ingest_github(
    cfg: &Config,
    repo: &str,
    include_source: bool,
    reporter: PhaseReporter,
) -> Result<usize> {
    log_info(&format!("command=ingest source=github repo={repo}"));
    let target =
        parse_github_target(repo).ok_or_else(|| anyhow::anyhow!("invalid GitHub repo: {repo}"))?;

    let octo = build_octocrab(cfg)?;
    let repo_info = octo.repos(&target.owner, &target.repo).get().await?;
    let default_branch = repo_info
        .default_branch
        .as_deref()
        .unwrap_or("main")
        .to_string();

    let common = GitHubCommonFields {
        repo_slug: target.repo_slug,
        owner: target.owner.clone(),
        name: target.repo.clone(),
        default_branch,
        repo_description: repo_info.description.clone(),
        pushed_at: repo_info.pushed_at.map(|dt| dt.to_rfc3339()),
        is_private: repo_info.private,
        has_wiki: repo_info.has_wiki.unwrap_or(false),
    };

    reporter
        .report(serde_json::json!({
            "phase": "ingesting",
            "tasks_total": GITHUB_SUBTASK_COUNT,
            "tasks_done": 0,
        }))
        .await;

    log_info(&format!(
        "github tasks_start repo={} has_wiki={} include_source={include_source}",
        common.repo_slug, common.has_wiki
    ));

    use tokio::time::timeout;
    let results = timeout(
        std::time::Duration::from_secs(GITHUB_INGEST_TOTAL_TIMEOUT_SECS),
        run_github_subtasks(cfg, &common, &repo_info, &octo, include_source, &reporter),
    )
    .await
    .unwrap_or_else(|_| {
        log_warn(&format!(
            "github ingest timed out after {GITHUB_INGEST_TOTAL_TIMEOUT_SECS}s repo={}",
            common.repo_slug
        ));
        [
            ("files", Err(anyhow::anyhow!("timed out"))),
            ("metadata", Err(anyhow::anyhow!("timed out"))),
            ("issues", Err(anyhow::anyhow!("timed out"))),
            ("prs", Err(anyhow::anyhow!("timed out"))),
            ("wiki", Err(anyhow::anyhow!("timed out"))),
        ]
    });
    let (total, issues_count, prs_count) = tally_results(results, repo);

    reporter
        .report(serde_json::json!({
            "tasks_done": GITHUB_SUBTASK_COUNT,
            "tasks_total": GITHUB_SUBTASK_COUNT,
            "chunks_embedded": total,
            "phase": "completed",
        }))
        .await;

    log_info(&format!(
        "github issues_fetched={issues_count} prs_fetched={prs_count}"
    ));
    log_done(&format!(
        "command=ingest source=github repo={repo} chunk_count={total}"
    ));
    Ok(total)
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "github/tests.rs"]
mod tests;

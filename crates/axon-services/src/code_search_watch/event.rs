use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CodeSearchWatchEvent {
    Started {
        watch_dirs: Vec<PathBuf>,
        roots: Vec<PathBuf>,
        initial_refresh: bool,
    },
    RefreshStarted {
        root: PathBuf,
        reason: &'static str,
    },
    RefreshFinished {
        root: PathBuf,
        status: String,
        warning: Option<String>,
        indexed_files: usize,
        removed_files: usize,
        generation: Option<i64>,
    },
    RefreshFailed {
        root: PathBuf,
        error: String,
    },
    Pending {
        root: PathBuf,
        paths: usize,
    },
    DryRun {
        plan: CodeSearchWatchDryRunPlan,
    },
    Enabled {
        unit_path: PathBuf,
        env_path: PathBuf,
        roots: Vec<PathBuf>,
    },
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodeSearchWatchDryRunPlan {
    pub roots: Vec<CodeSearchWatchDryRunRoot>,
    pub total_files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodeSearchWatchDryRunRoot {
    pub root: PathBuf,
    pub files: Vec<String>,
}

pub trait CodeSearchWatchEventSink: Sync {
    fn emit(&self, event: CodeSearchWatchEvent);
}

pub struct StdoutCodeSearchWatchEventSink {
    pub json: bool,
}

impl CodeSearchWatchEventSink for StdoutCodeSearchWatchEventSink {
    fn emit(&self, event: CodeSearchWatchEvent) {
        if self.json {
            emit_json(event);
        } else {
            emit_text(event);
        }
    }
}

fn emit_json(event: CodeSearchWatchEvent) {
    match serde_json::to_string(&event) {
        Ok(line) => println!("{line}"),
        Err(error) => println!(
            "{{\"event\":\"serialization_failed\",\"error\":{}}}",
            serde_json::json!(error.to_string())
        ),
    }
}

fn emit_text(event: CodeSearchWatchEvent) {
    match event {
        CodeSearchWatchEvent::Started {
            watch_dirs,
            roots,
            initial_refresh,
        } => println!(
            "code-search watcher started: {} watch dir(s), {} repo{}{}",
            watch_dirs.len(),
            roots.len(),
            if roots.len() == 1 { "" } else { "s" },
            if initial_refresh {
                " (initial refresh enabled)"
            } else {
                ""
            }
        ),
        CodeSearchWatchEvent::RefreshStarted { root, reason } => {
            println!("code-search refresh started: {} {reason}", root.display());
        }
        CodeSearchWatchEvent::RefreshFinished {
            root,
            status,
            warning,
            indexed_files,
            removed_files,
            generation,
        } => emit_refresh_finished(
            root,
            status,
            warning,
            indexed_files,
            removed_files,
            generation,
        ),
        CodeSearchWatchEvent::RefreshFailed { root, error } => {
            println!("code-search refresh failed: {} {error}", root.display());
        }
        CodeSearchWatchEvent::Pending { root, paths } => {
            println!(
                "code-search watcher queued changes: {} {paths} path(s)",
                root.display()
            );
        }
        CodeSearchWatchEvent::DryRun { plan } => emit_dry_run(plan),
        CodeSearchWatchEvent::Enabled {
            unit_path,
            env_path,
            roots,
        } => {
            println!(
                "code-search watcher enabled: {} repo root(s), unit={}, env={}",
                roots.len(),
                unit_path.display(),
                env_path.display()
            );
        }
        CodeSearchWatchEvent::Stopped => println!("code-search watcher stopped"),
    }
}

fn emit_refresh_finished(
    root: PathBuf,
    status: String,
    warning: Option<String>,
    indexed_files: usize,
    removed_files: usize,
    generation: Option<i64>,
) {
    let generation = generation
        .map(|generation| generation.to_string())
        .unwrap_or_else(|| "none".to_string());
    println!(
        "code-search refresh finished: {} status={status} indexed={indexed_files} removed={removed_files} generation={generation}",
        root.display()
    );
    if let Some(warning) = warning {
        println!("code-search refresh warning: {warning}");
    }
}

fn emit_dry_run(plan: CodeSearchWatchDryRunPlan) {
    println!(
        "code-search dry-run: {} repo(s), {} file(s)",
        plan.roots.len(),
        plan.total_files
    );
    for root in plan.roots {
        println!("{}", root.root.display());
        for file in root.files {
            println!("  {file}");
        }
    }
}

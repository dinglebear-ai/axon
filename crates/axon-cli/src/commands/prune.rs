//! CLI wrapper for `axon prune plan|exec`
//! (`docs/pipeline-unification/surfaces/command-contract.md`).
//!
//! Thin shim over `axon_services::prune` — the CLI owns only target parsing,
//! the destructive confirmation gate, and output formatting. Prune is
//! dry-run-only under `plan`; `exec` is the sole destructive path and always
//! requires `--confirm`.
//!
//! `--confirm` is necessary but not sufficient: `axon_services::prune::prune`
//! also requires the resolved [`PruneAuthz`] to hold admin. The CLI is a
//! locally-trusted process (no OAuth token in play), so a caller who can run
//! `axon prune exec --confirm` on this host is treated as admin — mirroring
//! how `axon reset --yes` needs no separate scope check. MCP/REST callers
//! must derive `PruneAuthz` from their real bearer/OAuth scopes instead of
//! reusing this local-trust shortcut.
use axon_api::source::ids::{SourceGenerationId, SourceId};
use axon_api::source::prune::{PruneRequest, PruneResult, PruneSelector};
use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_core::ui::{accent, muted, primary, symbol_for_status};
use axon_services::context::ServiceContext;
use axon_services::prune::{PruneAuthz, prune, prune_execute_saved};
use std::error::Error;

const COLLECTION_PREFIX: &str = "collection:";

pub async fn run_prune(cfg: &Config, ctx: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let subaction = cfg
        .positional
        .first()
        .map(String::as_str)
        .ok_or("prune requires a subcommand: plan|exec")?;

    if subaction == "exec" {
        if !cfg.prune_confirm {
            return Err("prune exec requires --confirm to run destructively".into());
        }
        let plan_id = cfg
            .prune_target
            .as_deref()
            .ok_or("prune exec requires the plan id returned by `prune plan`")?;
        let (plan, result, receipt) =
            prune_execute_saved(ctx, plan_id, true, &PruneAuthz::admin()).await?;
        if cfg.json_output {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "subaction": "exec",
                    "plan": plan,
                    "result": result,
                    "receipt_path": receipt,
                }))?
            );
        } else {
            report_result(&result);
            println!("{}", muted(&format!("receipt: {receipt}")));
        }
        return Ok(());
    }

    let selector = build_selector(cfg)?;
    let request = match subaction {
        "plan" => PruneRequest::dry_run(selector, "axon prune plan"),
        "exec" => {
            unreachable!("exec handled above")
        }
        other => {
            return Err(format!("unknown prune subcommand '{other}' (expected plan|exec)").into());
        }
    };

    log_info(&format!(
        "command=prune subaction={subaction} dry_run={}",
        request.dry_run
    ));

    // CLI is a locally-trusted process: a caller who can invoke `prune exec
    // --confirm` on this host is treated as admin. See module docs.
    let authz = PruneAuthz::admin();
    let (plan, result) = prune(ctx, &request, &authz).await?;

    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "subaction": subaction,
                "plan": plan,
                "result": result,
            }))?
        );
        return Ok(());
    }

    match result {
        None => {
            println!(
                "{}  prune plan (dry-run — nothing was changed)",
                accent("▸")
            );
            println!("{}", muted(&format!("selector: {:?}", plan.selector)));
            println!("{}", muted(&format!("plan_id: {}", plan.job_id.0)));
            if plan.steps.is_empty() {
                println!(
                    "{}",
                    muted("no deletions are estimated for this reviewed plan")
                );
            }
            for step in &plan.steps {
                println!(
                    "  {} {:?}: ~{} item(s) — {}",
                    accent("•"),
                    step.target,
                    step.estimated_deletes,
                    step.description
                );
            }
            println!(
                "{}",
                muted(&format!(
                    "run `axon prune exec {} --confirm` to execute this plan.",
                    plan.job_id.0
                ))
            );
        }
        Some(result) => report_result(&result),
    }
    Ok(())
}

fn report_result(result: &PruneResult) {
    println!(
        "{} {} status={:?}",
        symbol_for_status("completed"),
        primary("prune"),
        result.status
    );
    println!(
        "{}",
        muted(&format!(
            "deleted: {} total (vector={}, artifacts={}, graph_nodes={}, graph_edges={}, memory={}, ledger_generations={}, jobs={}, cache={})",
            result.deleted_counts.total(),
            result.deleted_counts.vector_points,
            result.deleted_counts.artifacts,
            result.deleted_counts.graph_nodes,
            result.deleted_counts.graph_edges,
            result.deleted_counts.memory_records,
            result.deleted_counts.ledger_generations,
            result.deleted_counts.jobs,
            result.deleted_counts.cache_entries,
        ))
    );
    if result.cleanup_debt_remaining > 0 {
        println!(
            "{}",
            muted(&format!(
                "cleanup_debt_remaining: {}",
                result.cleanup_debt_remaining
            ))
        );
    }
}

/// Build a [`PruneSelector`] from `cfg.prune_target`/`cfg.prune_generation`.
///
/// `target` is either `collection:<name>` (whole-collection prune) or a bare
/// source id, optionally narrowed to one generation via `--generation`.
fn build_selector(cfg: &Config) -> Result<PruneSelector, Box<dyn Error>> {
    let target = cfg
        .prune_target
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or("prune requires a target (source id, or collection:<name>)")?;

    if let Some(collection) = target.strip_prefix(COLLECTION_PREFIX) {
        let collection = collection.trim();
        if collection.is_empty() {
            return Err("collection: target requires a non-empty collection name".into());
        }
        if cfg.prune_generation.is_some() {
            return Err("--generation is not valid with a collection: target".into());
        }
        return Ok(PruneSelector::Collection {
            collection: collection.to_string(),
        });
    }

    let source_id = SourceId::new(target);
    Ok(match cfg.prune_generation.as_deref().map(str::trim) {
        Some(generation) if !generation.is_empty() => PruneSelector::Generation {
            source_id,
            generation: SourceGenerationId::new(generation),
        },
        _ => PruneSelector::Source { source_id },
    })
}

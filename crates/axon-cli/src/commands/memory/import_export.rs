//! `axon memory import <path>` / `axon memory export [--output <path>]`.
//!
//! Both subcommands need typed request/response I/O that doesn't fit the
//! flat `MemoryRequest`/`dispatch` shape the rest of `memory` uses, so they
//! call [`axon_services::memory::import`]/[`export`] directly.

use axon_api::source::{MemoryExportRequest, MemoryImportMode, MemoryImportRequest, MemoryScope};
use axon_services::context::ServiceContext;
use axon_services::memory as memory_svc;
use std::error::Error;
use std::fs;

pub(super) async fn run_import(
    args: &[String],
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let path = args.get(1).ok_or("memory import requires a file path")?;
    let mut mode = MemoryImportMode::Merge;
    let mut dry_run = false;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" => {
                let value = args.get(i + 1).ok_or("--mode requires a value")?.as_str();
                mode = match value {
                    "merge" => MemoryImportMode::Merge,
                    "replace_scope" => MemoryImportMode::ReplaceScope,
                    other => return Err(format!("unknown import mode: {other}").into()),
                };
                i += 2;
            }
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            other => return Err(format!("unknown memory import option: {other}").into()),
        }
    }

    let raw = fs::read_to_string(path).map_err(|err| format!("failed to read {path}: {err}"))?;
    let records = serde_json::from_str(&raw)
        .map_err(|err| format!("failed to parse {path} as a memory record array: {err}"))?;

    // CLI is a local-trust transport (matching `axon prune`'s
    // `PruneAuthz::admin()` rationale in `axon-cli/src/commands/prune.rs`) —
    // there is no bearer/OAuth caller identity to derive scopes from, so
    // `replace_scope` is allowed the same way it always has been for local
    // CLI callers.
    let result = memory_svc::import(
        service_context,
        MemoryImportRequest {
            records,
            mode,
            dry_run,
        },
        &memory_svc::MemoryAuthz::admin(),
    )
    .await
    .map_err(|err| format!("memory import failed: {err}"))?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

pub(super) async fn run_export(
    args: &[String],
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let mut output: Option<String> = None;
    let mut scope: Option<MemoryScope> = None;
    let mut include_archived = false;
    let mut include_working = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--output" => {
                output = Some(args.get(i + 1).ok_or("--output requires a value")?.clone());
                i += 2;
            }
            "--scope-kind" => {
                let kind = args
                    .get(i + 1)
                    .ok_or("--scope-kind requires a value")?
                    .clone();
                let value = scope.as_ref().map(|s| s.value.clone()).unwrap_or_default();
                scope = Some(MemoryScope { kind, value });
                i += 2;
            }
            "--scope-value" => {
                let value = args
                    .get(i + 1)
                    .ok_or("--scope-value requires a value")?
                    .clone();
                let kind = scope.as_ref().map(|s| s.kind.clone()).unwrap_or_default();
                scope = Some(MemoryScope { kind, value });
                i += 2;
            }
            "--include-archived" => {
                include_archived = true;
                i += 1;
            }
            "--include-working" => {
                include_working = true;
                i += 1;
            }
            other => return Err(format!("unknown memory export option: {other}").into()),
        }
    }

    // CLI is a local-trust transport (matching `run_import`'s rationale
    // above): admin visibility so a local export is never silently missing
    // `sensitive` records the operator asked for.
    let result = memory_svc::export(
        service_context,
        MemoryExportRequest {
            scope,
            include_archived,
            include_working,
        },
        &memory_svc::MemoryAuthz::admin(),
    )
    .await
    .map_err(|err| format!("memory export failed: {err}"))?;

    let rendered = serde_json::to_string_pretty(&result)?;
    match output {
        Some(path) => {
            fs::write(&path, &rendered).map_err(|err| format!("failed to write {path}: {err}"))?;
        }
        None => println!("{rendered}"),
    }
    Ok(())
}

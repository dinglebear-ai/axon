//! Ancillary repo-derived outputs in the same refresh/check transaction order.
use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

#[path = "runtime_config_docs.rs"]
mod runtime_config_docs;

pub(super) fn run(root: &Path, check: bool) -> Result<()> {
    if check {
        crate::checks::api_parity::check(root)?;
        crate::checks::dep_graph::check(root)?;
        crate::checks::public_api::check(root)?;
    } else {
        crate::checks::api_parity::write(root)?;
        crate::checks::dep_graph::write(root)?;
        crate::checks::public_api::write(root)?;
    }
    runtime_config_docs::run(root, check)?;
    for script in [
        "scripts/generate_mcp_schema_doc.py",
        "scripts/generate_action_docs.py",
    ] {
        python(root, script, if check { Some("--check") } else { None })?;
    }
    python(
        root,
        "scripts/check-integration-contracts.py",
        if check { None } else { Some("--refresh") },
    )
}

fn python(root: &Path, script: &str, argument: Option<&str>) -> Result<()> {
    let mut command = Command::new("python3");
    command.current_dir(root).arg(root.join(script));
    if let Some(argument) = argument {
        command.arg(argument);
    }
    let result = command.output().with_context(|| {
        format!("run {script}; install Python 3 and retry the documentation stage")
    })?;
    print!("{}", String::from_utf8_lossy(&result.stdout));
    eprint!("{}", String::from_utf8_lossy(&result.stderr));
    if !result.status.success() {
        bail!(
            "{script} exited {}; earlier refresh stages may have written outputs. Fix the reported input and rerun cargo xtask generated-contracts refresh; do not hand-edit generated files",
            result.status
        );
    }
    Ok(())
}

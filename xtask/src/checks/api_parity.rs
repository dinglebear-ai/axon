//! Generates `docs/reference/api-parity.md` — the factual CLI/MCP/REST parity
//! matrix — from the actual source surfaces, so it can't go stale:
//!
//! - **CLI**: `CommandKind` variants (`crates/axon-core/.../enums.rs`)
//! - **MCP**: `AxonRequest` variants (`mcp_schema.rs`) minus the HTTP-only arm
//!   in `crates/axon-mcp/src/server.rs`
//! - **REST**: `/v1/*` paths in the generated `apps/web/openapi/axon.json`
//!
//! `gen-api-parity` writes the file; `check-api-parity` (run in `xtask check`)
//! regenerates and fails on drift.

use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const OUTPUT: &str = "docs/reference/api-parity.md";

fn pascal_to_snake(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Extract the variant identifiers from `pub enum <Name> { ... }` in `text`.
fn enum_variants(text: &str, enum_name: &str) -> Vec<String> {
    let mut variants = Vec::new();
    let needle = format!("pub enum {enum_name} {{");
    let Some(start) = text.find(&needle) else {
        return variants;
    };
    let body = &text[start + needle.len()..];
    let end = body.find("\n}").unwrap_or(body.len());
    for line in body[..end].lines() {
        let trimmed = line.trim();
        // A variant line looks like `Ident,` or `Ident(Type),` — take the leading
        // PascalCase identifier, skip comments/attributes.
        let ident: String = trimmed
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if ident.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            variants.push(ident);
        }
    }
    variants
}

/// The `AxonRequest::*` arms routed to "available through the HTTP API, not MCP".
fn mcp_excluded(server_rs: &str) -> BTreeSet<String> {
    let mut excluded = BTreeSet::new();
    let Some(marker) = server_rs.find("available through the HTTP API, not MCP") else {
        return excluded;
    };
    // Walk backwards from the marker. The HTTP-only arm is a run of
    // `| AxonRequest::X(_)` lines sharing one `=> { ... }`. The dispatch arms
    // above it each end in `=> self.handle_…`, which is the boundary.
    let prefix = &server_rs[..marker];
    let mut started = false;
    for line in prefix.lines().rev() {
        if line.contains("=> self") {
            // A dispatch arm — boundary of the HTTP-only run.
            if started {
                break;
            }
            continue;
        }
        if line.contains("AxonRequest::") {
            started = true;
            for token in line.split("AxonRequest::").skip(1) {
                let ident: String = token
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
                if !ident.is_empty() {
                    excluded.insert(pascal_to_snake(&ident));
                }
            }
        } else if started {
            break;
        }
    }
    excluded
}

fn rest_segments(axon_json: &str) -> Result<BTreeSet<String>> {
    let spec: serde_json::Value =
        serde_json::from_str(axon_json).context("parse apps/web/openapi/axon.json")?;
    let mut segs = BTreeSet::new();
    if let Some(paths) = spec.get("paths").and_then(|p| p.as_object()) {
        for path in paths.keys() {
            if let Some(seg) = path.strip_prefix("/v1/").and_then(|r| r.split('/').next())
                && !seg.is_empty()
            {
                segs.insert(seg.to_string());
            }
        }
    }
    Ok(segs)
}

fn render(root: &Path) -> Result<String> {
    let read = |rel: &str| -> Result<String> {
        std::fs::read_to_string(root.join(rel)).with_context(|| format!("read {rel}"))
    };
    let cli: BTreeSet<String> = enum_variants(
        &read("crates/axon-core/src/config/types/enums.rs")?,
        "CommandKind",
    )
    .iter()
    .map(|v| pascal_to_snake(v))
    .collect();
    let mcp_all: BTreeSet<String> =
        enum_variants(&read("crates/axon-api/src/mcp_schema.rs")?, "AxonRequest")
            .iter()
            .map(|v| pascal_to_snake(v))
            .collect();
    let excluded = mcp_excluded(&read("crates/axon-mcp/src/server.rs")?);
    let mcp: BTreeSet<String> = mcp_all.difference(&excluded).cloned().collect();
    let rest = rest_segments(&read("apps/web/openapi/axon.json")?)?;

    let mut all: BTreeSet<String> = BTreeSet::new();
    all.extend(cli.iter().cloned());
    all.extend(mcp.iter().cloned());
    all.extend(rest.iter().cloned());

    let mark = |present: bool| if present { "✓" } else { "—" };
    let mut counts: BTreeMap<&str, usize> =
        BTreeMap::from([("cli", 0), ("mcp", 0), ("rest", 0), ("all3", 0)]);
    let mut rows = String::new();
    for op in &all {
        let (c, m, r) = (cli.contains(op), mcp.contains(op), rest.contains(op));
        if c {
            *counts.get_mut("cli").unwrap() += 1;
        }
        if m {
            *counts.get_mut("mcp").unwrap() += 1;
        }
        if r {
            *counts.get_mut("rest").unwrap() += 1;
        }
        if c && m && r {
            *counts.get_mut("all3").unwrap() += 1;
        }
        rows.push_str(&format!(
            "| `{op}` | {} | {} | {} |\n",
            mark(c),
            mark(m),
            mark(r)
        ));
    }

    // Derive the REST-only callout from the computed sets so the narrative can't
    // drift from the matrix when a new client/server-only segment is added.
    let rest_only = all
        .iter()
        .filter(|op| rest.contains(*op) && !cli.contains(*op) && !mcp.contains(*op))
        .map(|op| format!("`{op}`"))
        .collect::<Vec<_>>()
        .join(", ");

    Ok(format!(
        "# API Parity Matrix\n\
         \n\
         <!-- GENERATED by `cargo xtask gen-api-parity` — DO NOT EDIT BY HAND. -->\n\
         <!-- Sources: CommandKind (CLI), AxonRequest minus the HTTP-only arm (MCP), apps/web/openapi/axon.json (REST). -->\n\
         <!-- Run `cargo xtask gen-api-parity` to regenerate; `cargo xtask check-api-parity` (in `xtask check`) fails on drift. -->\n\
         \n\
         Factual matrix of which operations are exposed on each control surface.\n\
         `✓` = exposed, `—` = not. {total} operations: {cli_n} CLI, {mcp_n} MCP, {rest_n} REST, {all3_n} on all three.\n\
         \n\
         | Operation | CLI | MCP | REST |\n\
         |---|:--:|:--:|:--:|\n\
         {rows}\n\
         \n\
         **Notes.** MCP intentionally omits destructive/stateful admin actions routed HTTP-only \
         (see the `AxonRequest` arm in `crates/axon-mcp/src/server.rs`). REST-only rows \
         ({rest_only}) are client/server surfaces with no \
         CLI/MCP command. CLI-only rows are local/dev commands (`serve`, `mcp`, `completions`, \
         `setup`, `config`, …). A gap here is not automatically a bug — but a *new* gap should be a \
         conscious decision. See [crate-ownership.md](../architecture/crate-ownership.md) for where \
         the shared logic behind each surface lives.\n",
        total = all.len(),
        cli_n = counts["cli"],
        mcp_n = counts["mcp"],
        rest_n = counts["rest"],
        all3_n = counts["all3"],
        rows = rows.trim_end(),
    ))
}

pub fn write(root: &Path) -> Result<()> {
    let content = render(root)?;
    std::fs::write(root.join(OUTPUT), &content).with_context(|| format!("write {OUTPUT}"))?;
    println!("Wrote {OUTPUT} ({} bytes).", content.len());
    Ok(())
}

pub fn check(root: &Path) -> Result<()> {
    let expected = render(root)?;
    let actual = std::fs::read_to_string(root.join(OUTPUT)).unwrap_or_default();
    if expected == actual {
        println!("OK: {OUTPUT} is in sync.");
        return Ok(());
    }
    eprintln!("ERROR: {OUTPUT} is out of date.");
    eprintln!("Run `cargo xtask gen-api-parity` and commit the result.");
    bail!("api-parity drift");
}

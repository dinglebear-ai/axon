use axon_core::content::redact_url;
use axon_core::redact::redact_secrets;
use axon_core::ui::{muted, primary, status_text, symbol_for_status};

fn report_value<'a>(report: &'a serde_json::Value, path: &[&str]) -> &'a serde_json::Value {
    path.iter().fold(report, |curr, key| {
        curr.get(*key).unwrap_or(&serde_json::Value::Null)
    })
}

fn report_has(report: &serde_json::Value, path: &[&str]) -> bool {
    !report_value(report, path).is_null()
}

pub(crate) fn report_bool(report: &serde_json::Value, path: &[&str]) -> bool {
    report_value(report, path).as_bool().unwrap_or(false)
}

/// Extracts a string field from a doctor/status/debug report and redacts any
/// secret-shaped substrings (tokens, API keys, passwords) via the shared
/// redaction boundary (D1-09). Every render call site pulls its display text
/// through this helper, so this is the single choke point that keeps secrets
/// out of human-readable doctor/status/debug output — service `detail`
/// strings, config diagnostics, and LLM-echoed text may all carry raw
/// upstream error bodies or env values.
pub(crate) fn report_text(report: &serde_json::Value, path: &[&str], default: &str) -> String {
    let raw = report_value(report, path).as_str().unwrap_or(default);
    redact_secrets(raw)
}

pub(crate) fn report_i64(report: &serde_json::Value, path: &[&str]) -> i64 {
    report_value(report, path).as_i64().unwrap_or(0)
}

fn status_from_bool(ok: bool) -> &'static str {
    if ok { "completed" } else { "failed" }
}

fn render_status_line(name: &str, ok: bool, detail: &str) {
    let status = status_from_bool(ok);
    println!(
        "  {} {} {} {}",
        symbol_for_status(status),
        name,
        status_text(status),
        muted(detail),
    );
}

/// Like `render_status_line` but for optional services: uses a neutral `·` symbol
/// when the service is not configured so it doesn't look like a failure.
fn render_optional_status_line(name: &str, configured: bool, ok: bool, detail: &str) {
    if !configured {
        println!("  {} {} {}", muted("·"), name, muted(detail));
    } else {
        render_status_line(name, ok, detail);
    }
}

fn render_tei_info_lines(report: &serde_json::Value) {
    if let Some(url) = report["services"]["tei"]["url"].as_str()
        && !url.is_empty()
    {
        println!("    url: {}", muted(url));
    }
    if let Some(model) = report["services"]["tei"]["model"].as_str() {
        println!("    model: {}", muted(model));
    }
    if let Some(summary) = report["services"]["tei"]["summary"].as_str() {
        println!("    info: {}", muted(summary));
    } else if let Some(detail) = report["services"]["tei"]["info_detail"].as_str() {
        println!("    info: {}", muted(detail));
    }
}

fn chrome_status_label(report: &serde_json::Value) -> String {
    if report_bool(report, &["services", "chrome", "configured"]) {
        let url = report_text(report, &["services", "chrome", "url"], "");
        let detail = report_text(report, &["services", "chrome", "detail"], "unreachable");
        format!("{} ({})", redact_url(&url), detail)
    } else {
        "not configured (optional)".to_string()
    }
}

fn render_sqlite_service_line(report: &serde_json::Value) {
    let path = report_text(report, &["services", "sqlite", "path"], "unknown");
    let exists = report_bool(report, &["services", "sqlite", "exists"]);
    let ok = report_bool(report, &["services", "sqlite", "ok"]);
    let quick_check = report_text(report, &["services", "sqlite", "quick_check"], "unknown");
    let corrupted_count = report_i64(report, &["services", "sqlite", "corrupted_count"]);
    let ioerr_count = report_i64(report, &["services", "sqlite", "runtime_ioerr_count"]);
    let active_owner = report_bool(report, &["services", "sqlite", "active_owner_observed"]);
    let detail = if exists {
        format!(
            "path={path} quick_check={quick_check} active_owner={active_owner} corrupted_sidecars={corrupted_count} runtime_ioerr={ioerr_count}"
        )
    } else {
        format!("path={path} (will be created on first use)")
    };
    render_status_line("sqlite", ok, &detail);
}

fn render_services_section(report: &serde_json::Value) {
    println!("{}", primary("Services"));

    render_sqlite_service_line(report);

    let tei_ok = report_bool(report, &["services", "tei", "ok"]);
    let qdrant_ok = report_bool(report, &["services", "qdrant", "ok"]);
    let chrome_ok = report_bool(report, &["services", "chrome", "ok"]);

    render_status_line(
        "tei",
        tei_ok,
        &report_text(report, &["services", "tei", "detail"], "unreachable"),
    );
    render_tei_info_lines(report);
    render_status_line(
        "qdrant",
        qdrant_ok,
        &report_text(report, &["services", "qdrant", "url"], "n/a"),
    );
    let chrome_configured = report_bool(report, &["services", "chrome", "configured"]);
    render_optional_status_line(
        "chrome",
        chrome_configured,
        chrome_ok,
        &chrome_status_label(report),
    );
    if report_has(report, &["services", "gemini_headless"]) {
        render_status_line(
            "gemini_headless",
            report_bool(report, &["services", "gemini_headless", "ok"]),
            &report_text(
                report,
                &["services", "gemini_headless", "detail"],
                "not configured",
            ),
        );
    }
    if report_has(report, &["services", "openai"]) {
        let openai_configured = report_bool(report, &["services", "openai", "configured"]);
        render_optional_status_line(
            "openai",
            openai_configured,
            report_bool(report, &["services", "openai", "ok"]),
            &report_text(report, &["services", "openai", "detail"], "not configured"),
        );
    }
}

fn render_pipeline_row(report: &serde_json::Value, name: &str) {
    let ok = report_bool(report, &["pipelines", name]);
    let status = status_from_bool(ok);
    let queue = report_text(report, &["queue_names", name], "");
    let queue_label = if queue.is_empty() {
        String::new()
    } else {
        format!(" {}", muted(&format!("({})", queue)))
    };
    println!(
        "  {} {} {}{}",
        symbol_for_status(status),
        name,
        status_text(status),
        queue_label,
    );
}

fn render_pipelines_section(report: &serde_json::Value) {
    println!("{}", primary("Pipelines"));
    for name in ["source", "extract", "watch", "prune"] {
        render_pipeline_row(report, name);
    }
    // Extra warning line for extract when infra is up but LLM is missing.
    if report_bool(report, &["pipelines", "extract"])
        && !report_bool(report, &["pipelines", "extract_llm_ready"])
    {
        println!(
            "    {} LLM backend not ready — extract jobs will fail at LLM step",
            muted("⚠"),
        );
    }
}

fn render_stale_jobs_section(report: &serde_json::Value) {
    let stale = report_i64(report, &["stale_jobs"]);
    let pending = report_i64(report, &["pending_jobs"]);
    if stale > 0 || pending > 0 {
        println!();
        println!("{}", primary("Job Backlog"));
        if stale > 0 {
            println!(
                "  {} {} job(s) stuck in running >15 min — consider `axon jobs recover`",
                symbol_for_status("failed"),
                stale,
            );
        }
        if pending > 0 {
            println!(
                "  {} {} job(s) pending — are workers running?",
                muted("·"),
                pending,
            );
        }
    }
}

fn diagnostics_enabled_label(report: &serde_json::Value) -> &'static str {
    if report_bool(report, &["browser_runtime", "diagnostics", "enabled"]) {
        "enabled"
    } else {
        "disabled"
    }
}

fn render_browser_runtime_section(report: &serde_json::Value) {
    println!("{}", primary("Browser Runtime"));
    let on_off = |b: bool| if b { "on" } else { "off" };
    println!(
        "  diagnostics: {} (screenshot={} events={} dir={})",
        muted(diagnostics_enabled_label(report)),
        on_off(report_bool(
            report,
            &["browser_runtime", "diagnostics", "screenshot"]
        )),
        on_off(report_bool(
            report,
            &["browser_runtime", "diagnostics", "events"]
        )),
        report_text(
            report,
            &["browser_runtime", "diagnostics", "output_dir"],
            "."
        ),
    );
}

fn render_cutover_stores_section(report: &serde_json::Value) {
    let sqlite_non_empty =
        report_bool(report, &["cutover_stores", "stores", "sqlite", "non_empty"]);
    let vectors_non_empty = report_bool(
        report,
        &["cutover_stores", "stores", "vectors", "non_empty"],
    );
    let vectors_incompatible = report_bool(
        report,
        &["cutover_stores", "stores", "vectors", "schema_incompatible"],
    );
    let reset_recommended = report_bool(report, &["reset_recommended"]);

    println!("{}", primary("Cutover Stores"));
    let empty_status = |non_empty: bool, extra: &str| {
        if non_empty {
            format!("non-empty{extra}")
        } else {
            "empty/fresh".to_string()
        }
    };
    println!(
        "  {} sqlite {}",
        symbol_for_status(status_from_bool(!sqlite_non_empty)),
        muted(&empty_status(sqlite_non_empty, "")),
    );
    let vectors_extra = if vectors_incompatible {
        " (incompatible payload contract)"
    } else {
        ""
    };
    let vectors_ok = !vectors_incompatible;
    println!(
        "  {} vectors {}",
        symbol_for_status(status_from_bool(vectors_ok)),
        muted(&empty_status(
            vectors_non_empty || vectors_incompatible,
            vectors_extra
        )),
    );
    if reset_recommended {
        let guidance = report_text(report, &["cutover_stores", "guidance"], "run `axon reset`");
        println!("  {} {}", muted("⚠"), muted(&guidance));
    } else {
        println!(
            "  {} no incompatible cutover stores detected — no reset needed",
            muted("·")
        );
    }
}

fn render_config_diagnostics_section(report: &serde_json::Value) {
    let Some(diagnostics) = report["config_diagnostics"].as_array() else {
        return;
    };
    if diagnostics.is_empty() {
        return;
    }
    println!();
    println!("{}", primary("Config Diagnostics"));
    for item in diagnostics {
        let key = item["key"].as_str().unwrap_or("?");
        let message = item["message"].as_str().unwrap_or("");
        let remediation = item["remediation"].as_str().unwrap_or("");
        println!("  {} {}: {}", muted("⚠"), key, message);
        if !remediation.is_empty() {
            println!("    {} {}", muted("→"), muted(remediation));
        }
    }
}

pub(crate) fn render_doctor_report_human(report: &serde_json::Value) {
    let all_ok = report_bool(report, &["all_ok"]);

    println!("{}", primary("Doctor Report"));
    println!();
    render_services_section(report);

    println!();
    render_pipelines_section(report);

    render_stale_jobs_section(report);

    println!();
    render_cutover_stores_section(report);

    println!();
    render_browser_runtime_section(report);

    render_config_diagnostics_section(report);

    println!();
    let status = status_from_bool(all_ok);
    println!(
        "{} overall {}",
        symbol_for_status(status),
        status_text(status),
    );
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;

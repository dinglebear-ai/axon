use crate::core::content::redact_url;
use crate::core::ui::{muted, primary, status_text, symbol_for_status};

fn report_value<'a>(report: &'a serde_json::Value, path: &[&str]) -> &'a serde_json::Value {
    path.iter().fold(report, |curr, key| {
        curr.get(*key).unwrap_or(&serde_json::Value::Null)
    })
}

pub(crate) fn report_bool(report: &serde_json::Value, path: &[&str]) -> bool {
    report_value(report, path).as_bool().unwrap_or(false)
}

pub(crate) fn report_text(report: &serde_json::Value, path: &[&str], default: &str) -> String {
    report_value(report, path)
        .as_str()
        .unwrap_or(default)
        .to_string()
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
    let detail = if exists {
        format!("path={path}")
    } else {
        format!("path={path} (will be created on first use)")
    };
    render_status_line("sqlite", true, &detail);
}

fn render_services_section(report: &serde_json::Value) {
    println!("{}", primary("Services"));

    let lite = report_bool(report, &["lite_mode"]);
    if lite {
        render_sqlite_service_line(report);
    } else {
        let postgres_ok = report_bool(report, &["services", "postgres", "ok"]);
        let redis_ok = report_bool(report, &["services", "redis", "ok"]);
        let amqp_ok = report_bool(report, &["services", "amqp", "ok"]);
        render_status_line(
            "postgres",
            postgres_ok,
            &redact_url(&report_text(
                report,
                &["services", "postgres", "url"],
                "n/a",
            )),
        );
        render_status_line(
            "redis",
            redis_ok,
            &redact_url(&report_text(report, &["services", "redis", "url"], "n/a")),
        );
        render_status_line(
            "amqp",
            amqp_ok,
            &redact_url(&report_text(report, &["services", "amqp", "url"], "n/a")),
        );
    }

    let tei_ok = report_bool(report, &["services", "tei", "ok"]);
    let qdrant_ok = report_bool(report, &["services", "qdrant", "ok"]);
    let chrome_ok = report_bool(report, &["services", "chrome", "ok"]);
    let openai_ok = report_bool(report, &["services", "openai", "ok"]);

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
    let openai_configured = report_bool(report, &["services", "openai", "configured"]);
    render_optional_status_line(
        "openai",
        openai_configured,
        openai_ok,
        &report_text(report, &["services", "openai", "detail"], "not configured"),
    );
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
    for name in ["crawl", "extract", "embed", "ingest"] {
        render_pipeline_row(report, name);
    }
    // Extra warning line for extract when infra is up but LLM is missing.
    if report_bool(report, &["pipelines", "extract"])
        && !report_bool(report, &["pipelines", "extract_llm_ready"])
    {
        println!(
            "    {} openai not configured — extract jobs will fail at LLM step",
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
                "  {} {} job(s) stuck in running >15 min — consider `axon crawl recover`",
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

pub(crate) fn render_doctor_report_human(report: &serde_json::Value) {
    let all_ok = report_bool(report, &["all_ok"]);

    println!("{}", primary("Doctor Report"));
    println!();
    render_services_section(report);

    println!();
    render_pipelines_section(report);

    render_stale_jobs_section(report);

    println!();
    render_browser_runtime_section(report);

    println!();
    let status = status_from_bool(all_ok);
    println!(
        "{} overall {}",
        symbol_for_status(status),
        status_text(status),
    );
}

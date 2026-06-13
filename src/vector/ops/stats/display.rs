use crate::core::ui::{accent, muted, primary, status_text};

const STATS_TEXT_DISPLAY_LIMIT: usize = 120;
const STATS_CONTINUATION_INDENT: usize = 4;

fn fmt_count(v: &serde_json::Value) -> String {
    accent(
        &v.as_i64()
            .map(|n| n.to_string())
            .unwrap_or_else(|| "n/a".to_string()),
    )
}

pub(crate) fn print_stats_human(stats: &serde_json::Value) {
    print_vector_stats(stats);
    println!();
    print_pipeline_stats(stats);
    println!();
    print_freshness_stats(stats);
    println!();
    print_command_counts(stats);
}

fn print_vector_stats(stats: &serde_json::Value) {
    println!("{}", primary("Vector Stats"));
    println!(
        "  {} {}",
        muted("Collection:"),
        accent(stats["collection"].as_str().unwrap_or("unknown"))
    );
    println!(
        "  {} {}",
        muted("Status:"),
        status_text(stats["status"].as_str().unwrap_or("unknown"))
    );
    println!(
        "  {} {}",
        muted("Indexed Vectors:"),
        fmt_count(&stats["indexed_vectors_count"])
    );
    println!(
        "  {} {}",
        muted("Points:"),
        fmt_count(&stats["points_count"])
    );
    println!(
        "  {} {}",
        muted("Docs (est):"),
        fmt_count(&stats["docs_embedded_estimate"])
    );
    println!(
        "  {} {}",
        muted("Avg Chunks/Doc:"),
        accent(&format!(
            "{:.2}",
            stats["avg_chunks_per_doc"].as_f64().unwrap_or(0.0)
        ))
    );
    println!(
        "  {} {}",
        muted("Avg Chunk Tokens (est):"),
        accent(&avg_stat_text(stats, "avg_chunk_tokens_estimate", ""))
    );
    println!(
        "  {} {}",
        muted("Avg Doc Tokens (est):"),
        accent(&avg_stat_text(stats, "avg_doc_tokens_estimate", ""))
    );
    println!(
        "  {} {}",
        muted("Dimension:"),
        fmt_count(&stats["dimension"])
    );
    println!(
        "  {} {}",
        muted("Distance:"),
        stats["distance"].as_str().unwrap_or("unknown")
    );
    println!(
        "  {} {}",
        muted("Segments:"),
        fmt_count(&stats["segments_count"])
    );
    println!(
        "  {} {}",
        muted("Payload Fields:"),
        fmt_count(&stats["payload_fields_count"])
    );
    if let Some(rendered) = render_payload_fields(stats) {
        println!("  {}", muted("Field Names:"));
        for line in rendered.lines() {
            println!("    {}", line);
        }
    }
}

fn render_payload_fields(stats: &serde_json::Value) -> Option<String> {
    let fields = stats["payload_fields"]
        .as_array()?
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    if fields.is_empty() {
        None
    } else {
        Some(wrap_comma_list(
            &fields,
            STATS_TEXT_DISPLAY_LIMIT.saturating_sub(STATS_CONTINUATION_INDENT),
        ))
    }
}

fn wrap_comma_list(items: &[&str], max_chars: usize) -> String {
    let mut lines = Vec::new();
    let mut current = String::new();
    for item in items {
        let rendered = truncate_stats_text(item, max_chars);
        let separator = if current.is_empty() { "" } else { ", " };
        let candidate_len =
            current.chars().count() + separator.chars().count() + rendered.chars().count();
        if !current.is_empty() && candidate_len > max_chars {
            lines.push(current);
            current = rendered;
        } else {
            current.push_str(separator);
            current.push_str(&rendered);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines.join("\n")
}

fn truncate_stats_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    format!("{}…", truncate_chars(text, max_chars - 1))
}

fn truncate_chars(s: &str, max_chars: usize) -> &str {
    s.char_indices().nth(max_chars).map_or(s, |(i, _)| &s[..i])
}

fn avg_stat_text(stats: &serde_json::Value, key: &str, suffix: &str) -> String {
    stats[key]
        .as_f64()
        .map(|v| format!("{v:.2}{suffix}"))
        .unwrap_or_else(|| "n/a".to_string())
}

fn print_pipeline_stats(stats: &serde_json::Value) {
    println!("{}", primary("Pipeline Stats"));
    let avg_pages = avg_stat_text(stats, "avg_pages_crawled_per_second", "");
    let avg_crawl = avg_stat_text(stats, "avg_crawl_duration_seconds", "s");
    let avg_embed = avg_stat_text(stats, "avg_embedding_duration_seconds", "s");
    let avg_overall = avg_stat_text(stats, "avg_overall_crawl_duration_seconds", "s");
    println!("  {} {}", muted("Avg Pages/sec:"), accent(&avg_pages));
    println!("  {} {}", muted("Avg Crawl Duration:"), accent(&avg_crawl));
    println!(
        "  {} {}",
        muted("Avg Embedding Duration:"),
        accent(&avg_embed)
    );
    println!("  {} {}", muted("Avg Overall Crawl:"), accent(&avg_overall));
    println!(
        "  {} {}",
        muted("Total Chunks:"),
        fmt_count(&stats["total_chunks"])
    );
    println!(
        "  {} {}",
        muted("Total Docs:"),
        fmt_count(&stats["total_docs"])
    );
    println!(
        "  {} {}",
        muted("Base URLs:"),
        fmt_count(&stats["base_urls_count"])
    );
    if let Some(longest) = stats["longest_crawl"].as_object() {
        println!(
            "  {} {} ({:.2}s)",
            muted("Longest Crawl:"),
            accent(longest.get("id").and_then(|v| v.as_str()).unwrap_or("n/a")),
            longest
                .get("seconds")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
        );
    }
    if let Some(most) = stats["most_chunks"].as_object() {
        println!(
            "  {} {} ({})",
            muted("Most Chunks:"),
            accent(
                most.get("embed_job_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("n/a")
            ),
            accent(
                &most
                    .get("chunks")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0)
                    .to_string()
            )
        );
    }
}

/// Format a duration in seconds as a human-readable age string.
fn fmt_age_secs(secs: i64) -> String {
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3_600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        let h = secs / 3_600;
        let m = (secs % 3_600) / 60;
        if m == 0 {
            format!("{h}h ago")
        } else {
            format!("{h}h {m}m ago")
        }
    } else {
        let d = secs / 86_400;
        let h = (secs % 86_400) / 3_600;
        if h == 0 {
            format!("{d}d ago")
        } else {
            format!("{d}d {h}h ago")
        }
    }
}

fn print_freshness_stats(stats: &serde_json::Value) {
    println!("{}", primary("Freshness"));
    let age_text = stats["freshness"]["last_indexed_secs_ago"]
        .as_i64()
        .map(fmt_age_secs)
        .unwrap_or_else(|| "n/a".to_string());
    println!("  {} {}", muted("Last Indexed:"), accent(&age_text));
    println!(
        "  {} {}",
        muted("Crawls (24h):"),
        fmt_count(&stats["freshness"]["crawls_last_24h"])
    );
    println!(
        "  {} {}",
        muted("Crawls (7d):"),
        fmt_count(&stats["freshness"]["crawls_last_7d"])
    );

    let Some(days) = stats["growth_7d"].as_array() else {
        return;
    };
    if days.is_empty() {
        return;
    }
    let max_chunks = days
        .iter()
        .filter_map(|d| d["chunks"].as_i64())
        .max()
        .unwrap_or(1)
        .max(1);
    println!();
    println!("{}", primary("Growth (last 7 days)"));
    for day in days {
        let date = day["date"].as_str().unwrap_or("?");
        let chunks = day["chunks"].as_i64().unwrap_or(0);
        let bar_len = (chunks as f64 / max_chunks as f64 * 20.0).round() as usize;
        let bar = "█".repeat(bar_len);
        println!(
            "  {}  {:<20}  {}",
            muted(date),
            accent(&bar),
            muted(&format!("{chunks} chunks"))
        );
    }
}

fn print_command_counts(stats: &serde_json::Value) {
    println!("{}", primary("Command Counts"));
    println!(
        "  {} {}",
        muted("Crawls:"),
        fmt_count(&stats["counts"]["crawls"])
    );
    println!(
        "  {} {}",
        muted("Embeds:"),
        fmt_count(&stats["counts"]["embeds"])
    );
    println!(
        "  {} {}",
        muted("Scrapes:"),
        fmt_count(&stats["counts"]["scrapes"])
    );
    println!(
        "  {} {}",
        muted("Extracts:"),
        fmt_count(&stats["counts"]["extracts"])
    );
    println!(
        "  {} {}",
        muted("Queries:"),
        fmt_count(&stats["counts"]["queries"])
    );
    println!(
        "  {} {}",
        muted("Asks:"),
        fmt_count(&stats["counts"]["asks"])
    );
    println!(
        "  {} {}",
        muted("Retrieves:"),
        fmt_count(&stats["counts"]["retrieves"])
    );
    println!(
        "  {} {}",
        muted("Evaluates:"),
        fmt_count(&stats["counts"]["evaluates"])
    );
    println!(
        "  {} {}",
        muted("Suggests:"),
        fmt_count(&stats["counts"]["suggests"])
    );
    println!(
        "  {} {}",
        muted("Maps:"),
        fmt_count(&stats["counts"]["maps"])
    );
    println!(
        "  {} {}",
        muted("Searches:"),
        fmt_count(&stats["counts"]["searches"])
    );
}

#[cfg(test)]
#[path = "display_tests.rs"]
mod tests;

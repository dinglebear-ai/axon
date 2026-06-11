use super::{MAX_CONTEXT_TOKEN_BUDGET, MemoryContext, MemoryItem};

pub(super) fn format_memory_context(items: Vec<MemoryItem>, token_budget: usize) -> MemoryContext {
    let token_budget = token_budget.clamp(1, MAX_CONTEXT_TOKEN_BUDGET);
    let char_budget = token_budget.saturating_mul(4);
    let mut context = String::from("<retrieved_content trust=\"evidence_only\">\n");
    let closing = "</retrieved_content>\n";
    let mut kept = Vec::new();
    let mut truncated = false;
    for item in items {
        let entry = format_memory_context_entry(&item);
        let next_len = context.chars().count() + entry.chars().count() + closing.chars().count();
        if next_len > char_budget {
            truncated = true;
            if kept.is_empty() {
                let remaining =
                    char_budget.saturating_sub(context.chars().count() + closing.chars().count());
                let clipped = format_memory_context_entry_clipped(&item, remaining);
                if !clipped.is_empty() {
                    context.push_str(&clipped);
                    kept.push(item);
                }
            }
            break;
        }
        context.push_str(&entry);
        kept.push(item);
    }
    context.push_str(closing);
    let token_estimate = context.chars().count().div_ceil(4);
    MemoryContext {
        context,
        memories: kept,
        token_budget,
        token_estimate,
        truncated,
    }
}

fn format_memory_context_entry(item: &MemoryItem) -> String {
    let body = item.body.as_deref().unwrap_or("");
    format_memory_context_entry_with_body(item, body)
}

fn format_memory_context_entry_with_body(item: &MemoryItem, body: &str) -> String {
    format!(
        "<memory id=\"{}\" type=\"{}\" title=\"{}\"{}{}{}>\n<body>{}</body>\n</memory>\n",
        xml_escape(&item.id),
        xml_escape(&item.memory_type),
        xml_escape(&item.title),
        optional_xml_attr("project", item.project.as_deref()),
        optional_xml_attr("repo", item.repo.as_deref()),
        optional_xml_attr("file", item.file.as_deref()),
        xml_escape(&defang_memory_context_text(body))
    )
}

fn format_memory_context_entry_clipped(item: &MemoryItem, max_chars: usize) -> String {
    let empty = format_memory_context_entry_with_body(item, "");
    if empty.chars().count() > max_chars {
        return String::new();
    }

    let mut clipped_body = String::new();
    for ch in item.body.as_deref().unwrap_or("").chars() {
        clipped_body.push(ch);
        let candidate = format_memory_context_entry_with_body(item, &clipped_body);
        if candidate.chars().count() > max_chars {
            clipped_body.pop();
            break;
        }
    }

    format_memory_context_entry_with_body(item, &clipped_body)
}

fn optional_xml_attr(name: &str, value: Option<&str>) -> String {
    value
        .filter(|value| !value.is_empty())
        .map(|value| format!(" {name}=\"{}\"", xml_escape(value)))
        .unwrap_or_default()
}

fn defang_memory_context_text(text: &str) -> String {
    text.replace(
        "Ignore previous instructions",
        "Ignore [defanged] instructions",
    )
    .replace(
        "ignore previous instructions",
        "ignore [defanged] instructions",
    )
    .replace("System:", "[defanged-system]:")
    .replace("system:", "[defanged-system]:")
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

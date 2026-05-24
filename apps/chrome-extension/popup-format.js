function formatAxonScrape(result, tab, fallbackUrl) {
  if (Array.isArray(result.results)) {
    const sections = result.results.map((item, index) => {
      const url = result.urls?.[index] || item.url || fallbackUrl || tab?.url || "";
      return formatAxonScrape(item, tab, url);
    });
    return sections.join("\n\n---\n\n");
  }

  const markdown = result.markdown || result.payload?.markdown || result.output || "";
  const url = result.url || fallbackUrl || tab?.url || "";
  const words = markdown.trim().split(/\s+/).filter(Boolean).length;

  return [
    `# Scrape`,
    "",
    `${badge("success", "ready")} ${words.toLocaleString()} words`,
    `Page: ${tab?.title || result.payload?.title || "Untitled page"}`,
    `URL: ${url}`,
    "",
    markdown || "(Axon returned no markdown.)"
  ].join("\n");
}

function formatSummary(result, tab, fallbackUrl) {
  const summary = result.summary || result.payload?.summary || "";
  const urls = result.urls || result.payload?.urls || [fallbackUrl || tab?.url || ""];
  const contextChars = result.context_chars || result.payload?.context_chars;

  return [
    `# Summary`,
    "",
    `${badge(summary ? "success" : "warn", summary ? "ready" : "empty")}`,
    `Page: ${tab?.title || "Untitled page"}`,
    `URL: ${urls[0] || fallbackUrl || tab?.url || ""}`,
    contextChars ? `Context: ${contextChars.toLocaleString()} chars` : "",
    "",
    summary || "(Axon returned no summary.)"
  ].filter(Boolean).join("\n");
}

function formatMap(result, tab, fallbackUrl) {
  const urls = result.urls || [];
  const total = result.total ?? urls.length;
  const source = result.map_source || "unknown";

  return [
    `# Map`,
    "",
    `${badge(urls.length ? "success" : "warn", `${total.toLocaleString()} discovered`)}`,
    `Page: ${tab?.title || "Untitled page"}`,
    `Start URL: ${result.url || fallbackUrl || tab?.url || ""}`,
    `Source: ${source}`,
    result.warning ? `Warning: ${result.warning}` : "",
    "",
    ...urls
  ].filter(Boolean).join("\n");
}

function formatChatAnswer(result, question, tab) {
  return [
    `# Chat: ${question}`,
    "",
    `${badge("info", "/v1/ask")}`,
    tab?.url ? `Current page: ${tab.url}` : "",
    "",
    result.answer
  ].filter(Boolean).join("\n");
}

function formatRetrieve(result, url) {
  const chunks = result.chunks || result.points || result.payload?.chunks || result.payload?.points || [];
  const lines = [`# Retrieve`, "", `${badge(chunks.length ? "success" : "warn", `${chunks.length} chunks`)}`, `URL: ${url}`, ""];

  if (!chunks.length) {
    lines.push(readableValue(result));
    return lines.join("\n");
  }

  chunks.slice(0, 12).forEach((chunk, index) => {
    const payload = chunk.payload || chunk;
    const text = payload.text || payload.chunk_text || payload.content || payload.markdown || readableValue(payload);
    lines.push(`## Chunk ${index + 1}`, "", `${badge("neutral", payload.url || payload.source_url || "stored context")}`, "", text, "");
  });

  return lines.join("\n").trim();
}

function formatQuery(result, query) {
  const hits = result.results || result.hits || result.payload?.results || result.payload?.hits || [];
  const lines = [`# Query`, "", `Query: ${query}`, `${badge(hits.length ? "success" : "warn", `${hits.length} hits`)}`, ""];

  if (!hits.length) {
    lines.push(readableValue(result));
    return lines.join("\n");
  }

  hits.slice(0, 8).forEach((hit, index) => {
    const payload = hit.payload || hit;
    const url = payload.url || payload.source_url || hit.url || "";
    const score = hit.score ?? hit.similarity ?? payload.score;
    const text = payload.text || payload.chunk_text || payload.content || payload.title || "";
    lines.push(`## Result ${index + 1}`);
    if (score != null) {
      lines.push(`${badge(scoreTone(score), `score ${Number(score).toFixed(3)}`)}`);
    }
    if (url) {
      lines.push(url);
    }
    if (text) {
      lines.push("", truncate(text, 900));
    } else {
      lines.push("", readableValue(hit));
    }
    lines.push("");
  });

  return lines.join("\n").trim();
}

function acceptedJobResult(title, result, kind) {
  const jobId = result.job_id || result.jobId || result.id || "";
  const output = [
    `# ${title.replace(/\s+queued$/i, "")}`,
    "",
    badge("info", "queued"),
    jobId ? `Job: \`${jobId}\`` : badge("warn", "job id unavailable"),
    result.status_url ? `Status: ${result.status_url}` : "",
    result.status ? `State: ${badge(toneForStatus(result.status), result.status)}` : ""
  ].filter(Boolean).join("\n");
  return {
    output,
    copyText: output,
    copiedMessage: `Copied Axon ${kind} job to clipboard.`,
    doneMessage: jobId ? `Queued ${kind} job ${jobId}.` : `Queued ${kind} job.`
  };
}

function formatGenericResult(title, result) {
  const normalized = String(title || "").toLowerCase();
  if (normalized.includes("doctor")) return formatDoctor(result);
  if (normalized.includes("sources")) return formatSources(result);
  if (normalized.includes("domains")) return formatDomains(result);
  if (normalized.includes("stats")) return formatStats(result);
  if (normalized.includes("status")) return formatStatusResult(result);
  if (normalized.includes("research")) return formatResearchResult(result);
  if (normalized.includes("search")) return formatSearchResult(result, "Search");
  if (normalized.includes("evaluate")) return formatEvaluateResult(result);
  if (normalized.includes("suggest")) return formatSuggestResult(result);
  if (normalized.includes("watch")) return formatWatchResult(result);
  if (normalized.includes("dedupe")) return formatDedupeResult(result);
  if (normalized.includes("migrate")) return formatMigrateResult(result);
  return [`# ${capitalize(title)}`, "", readableValue(result)].join("\n");
}

function unsupportedCliCommand(name) {
  const output = [
    `# ${name}`,
    "",
    `\`${name}\` is a local Axon CLI command, but this extension can only call Axon's browser-exposed HTTP API.`,
    "Use it in a shell, or add a server endpoint if we want it available from Chrome."
  ].join("\n");
  return {
    output,
    doneMessage: `${name} is CLI-only from the extension.`
  };
}

function formatJson(value) {
  return JSON.stringify(value, null, 2);
}

function payloadOf(result) {
  return result?.payload && typeof result.payload === "object" ? result.payload : result;
}

function formatDoctor(result) {
  const payload = payloadOf(result) || {};
  const services = payload.services || {};
  const pipelines = payload.pipelines || {};
  const lines = [
    "# Doctor",
    "",
    payload.all_ok === true ? `${badge("success", "all ok")} All systems reachable.` : `${badge("warn", "attention")} Some checks need attention.`,
    payload.observed_at_utc ? `Observed: ${formatTime(payload.observed_at_utc)}` : "",
    statLine("Pending jobs", payload.pending_jobs, payload.pending_jobs ? "warn" : "success")
  ].filter(Boolean);

  if (Object.keys(services).length) {
    lines.push("", "Services");
    Object.entries(services).forEach(([name, service]) => {
      lines.push(`- ${badge(service?.ok ? "success" : "error", service?.ok ? "ok" : "problem")} ${humanizeKey(name)}${service?.detail ? ` - ${service.detail}` : ""}`);
    });
  }

  if (Object.keys(pipelines).length) {
    lines.push("", "Pipelines");
    Object.entries(pipelines).forEach(([name, ok]) => {
      lines.push(`- ${badge(ok ? "success" : "error", ok ? "ready" : "blocked")} ${humanizeKey(name)}`);
    });
  }

  return lines.join("\n");
}

function formatSources(result) {
  const payload = payloadOf(result) || {};
  const sources = payload.sources || payload.urls || result.sources || [];
  const lines = ["# Sources", "", badge(sources.length ? "success" : "warn", `${sources.length.toLocaleString()} indexed`)];
  sources.slice(0, 30).forEach((source) => {
    const url = typeof source === "string" ? source : source.url || source.source_url || source.id || "";
    const count = source.chunk_count ?? source.chunks ?? source.points;
    lines.push(`- ${count != null ? `${badge("neutral", `${count} chunks`)} ` : ""}${url || readableInline(source)}`);
  });
  if (sources.length > 30) lines.push(`- ...and ${(sources.length - 30).toLocaleString()} more`);
  return lines.join("\n");
}

function formatDomains(result) {
  const payload = payloadOf(result) || {};
  const domains = payload.domains || result.domains || [];
  const lines = ["# Domains", "", badge(domains.length ? "success" : "warn", `${domains.length.toLocaleString()} indexed`)];
  domains.slice(0, 30).forEach((domain) => {
    const name = typeof domain === "string" ? domain : domain.domain || domain.host || domain.name || "";
    const chunks = domain.chunk_count ?? domain.chunks;
    const urls = domain.url_count ?? domain.urls;
    lines.push(`- ${name || readableInline(domain)} ${urls != null ? badge("info", `${urls} URLs`) : ""} ${chunks != null ? badge("neutral", `${chunks} chunks`) : ""}`.trim());
  });
  return lines.join("\n");
}

function formatStats(result) {
  const payload = payloadOf(result) || {};
  return ["# Stats", "", readableValue(payload)].join("\n");
}

function formatStatusResult(result) {
  const payload = payloadOf(result) || {};
  const jobs = payload.jobs || result.jobs || [];
  if (Array.isArray(jobs) && jobs.length) {
    const lines = ["# Status", "", badge(jobs.length ? "info" : "neutral", `${jobs.length.toLocaleString()} recent jobs`)];
    jobs.slice(0, 20).forEach((job) => lines.push(`- ${formatJobLine(job, job.kind || "job")}`));
    return lines.join("\n");
  }

  const jobGroups = Object.entries(payload).filter(([key, value]) => key.endsWith("_jobs") && Array.isArray(value));
  if (jobGroups.length) {
    const lines = ["# Status"];
    jobGroups.forEach(([key, group]) => {
      lines.push("", `## ${humanizeKey(key)}`, badge(group.length ? "info" : "neutral", `${group.length.toLocaleString()} jobs`));
      group.slice(0, 12).forEach((job) => lines.push(`- ${formatJobLine(job, key.replace(/^local_/, "").replace(/_jobs$/, ""))}`));
      if (group.length > 12) lines.push(`- ...and ${(group.length - 12).toLocaleString()} more`);
    });
    return lines.join("\n");
  }

  return ["# Status", "", readableValue(payload)].join("\n");
}

function formatSearchResult(result, title) {
  const payload = payloadOf(result) || {};
  const results = payload.results || result.results || [];
  const jobs = payload.crawl_jobs || result.crawl_jobs || [];
  const lines = [`# ${title}`, "", `${badge(results.length ? "success" : "warn", `${results.length.toLocaleString()} results`)}${jobs.length ? ` ${badge("info", `${jobs.length} crawls queued`)}` : ""}`];
  results.slice(0, 10).forEach((item, index) => {
    lines.push("", `${index + 1}. ${item.title || item.url || item.href || "Result"}`);
    if (item.url || item.href) lines.push(`   ${item.url || item.href}`);
    if (item.content || item.snippet || item.text) lines.push(`   ${truncate(item.content || item.snippet || item.text, 260)}`);
  });
  return lines.join("\n");
}

function formatResearchResult(result) {
  const payload = payloadOf(result) || {};
  const answer = payload.answer || payload.summary || payload.synthesis;
  return answer ? ["# Research", "", answer].join("\n") : formatSearchResult(result, "Research");
}

function formatEvaluateResult(result) {
  const payload = payloadOf(result) || {};
  const lines = ["# Evaluate", ""];
  if (payload.verdict) lines.push(`Verdict: ${badge(toneForVerdict(payload.verdict), payload.verdict)}`);
  ["accuracy", "relevance", "completeness", "specificity"].forEach((key) => {
    if (payload[key] != null) lines.push(`${humanizeKey(key)}: ${badge(scoreTone(payload[key]), payload[key])}`);
  });
  if (payload.explanation || payload.reasoning) lines.push("", payload.explanation || payload.reasoning);
  return lines.length > 2 ? lines.join("\n") : ["# Evaluate", "", readableValue(payload)].join("\n");
}

function formatSuggestResult(result) {
  const payload = payloadOf(result) || {};
  const suggestions = payload.suggestions || payload.urls || result.suggestions || [];
  const lines = ["# Suggest", "", badge(suggestions.length ? "success" : "warn", `${suggestions.length.toLocaleString()} suggestions`)];
  suggestions.slice(0, 20).forEach((suggestion) => {
    const url = typeof suggestion === "string" ? suggestion : suggestion.url || suggestion.href || "";
    const reason = suggestion.reason || suggestion.title || "";
    lines.push(`- ${url || readableInline(suggestion)}${reason ? ` - ${reason}` : ""}`);
  });
  return lines.join("\n");
}

function formatWatchResult(result) {
  const payload = payloadOf(result) || {};
  const watches = payload.watches || result.watches || [];
  if (!Array.isArray(watches) || !watches.length) return ["# Watch", "", readableValue(payload)].join("\n");
  const lines = ["# Watch", "", badge(watches.length ? "info" : "neutral", `${watches.length.toLocaleString()} watches`)];
  watches.slice(0, 20).forEach((watch) => {
    lines.push(`- ${badge(watch.enabled === false ? "warn" : "success", watch.enabled === false ? "paused" : "enabled")} ${watch.name || watch.id} every ${watch.every_seconds || "?"}s`);
  });
  return lines.join("\n");
}

function formatDedupeResult(result) {
  const payload = payloadOf(result) || {};
  return [
    "# Dedupe",
    "",
    statLine("Deleted", payload.deleted ?? payload.points_deleted, "warn"),
    statLine("Scanned", payload.scanned ?? payload.points_scanned, "info"),
    statLine("Groups", payload.groups ?? payload.duplicate_groups, "neutral")
  ].filter(Boolean).join("\n") || ["# Dedupe", "", readableValue(payload)].join("\n");
}

function formatMigrateResult(result) {
  const payload = payloadOf(result) || {};
  return [
    "# Migrate",
    "",
    payload.from && payload.to ? `${payload.from} -> ${payload.to}` : "",
    statLine("Points migrated", payload.points_migrated, "success"),
    statLine("Pages processed", payload.pages_processed, "info")
  ].filter(Boolean).join("\n") || ["# Migrate", "", readableValue(payload)].join("\n");
}

function jobSummary(result) {
  const job = result.job || payloadOf(result)?.job || payloadOf(result);
  if (!job || typeof job !== "object") return "";
  return [
    job.id ? `Job: ${job.id}` : "",
    job.url ? `URL: ${job.url}` : "",
    job.error_text ? `Error: ${job.error_text}` : "",
    job.updated_at ? `Updated: ${formatTime(job.updated_at)}` : ""
  ].filter(Boolean).join("\n");
}

function formatJobLine(job, kind) {
  const id = job.id ? String(job.id).slice(0, 8) : "";
  const status = job.status || job.state || (job.error_text ? "failed" : job.finished_at ? "completed" : job.started_at ? "running" : "pending");
  const result = job.result_json || {};
  const pages = result.pages_crawled ?? result.ok_pages ?? result.md_created ?? result.total_pages;
  const target = job.url || job.target || job.input_text || "";
  const parts = [
    badge(toneForStatus(status), status),
    kind,
    id ? `${id}` : "",
    pages != null ? `(${Number(pages).toLocaleString()} pages)` : "",
    target ? `- ${target}` : "",
    job.error_text ? `- ${job.error_text}` : ""
  ];
  return parts.filter(Boolean).join(" ");
}

function formatSingleJobStatus(result, jobId) {
  const job = result.job || payloadOf(result)?.job || payloadOf(result) || {};
  const status = job.status || job.state || (job.error_text ? "failed" : job.finished_at ? "completed" : job.started_at ? "running" : "pending");
  return [
    "# Job status",
    "",
    `${badge(toneForStatus(status), status)} ${job.kind || "crawl"} \`${job.id || jobId}\``,
    job.url ? `URL: ${job.url}` : "",
    job.created_at ? `Created: ${formatTime(job.created_at)}` : "",
    job.started_at ? `Started: ${formatTime(job.started_at)}` : "",
    job.finished_at ? `Finished: ${formatTime(job.finished_at)}` : "",
    job.error_text ? `${badge("error", "error")} ${job.error_text}` : "",
    job.result_json ? ["", "Result", readableValue(job.result_json)].join("\n") : ""
  ].filter(Boolean).join("\n");
}

function readableValue(value, depth = 0) {
  const indent = "  ".repeat(depth);
  if (value == null || typeof value !== "object") return formatPrimitive(value);
  if (Array.isArray(value)) {
    if (!value.length) return "None.";
    return value.slice(0, 12).map((item) => `${indent}- ${readableInline(item)}`).join("\n");
  }
  const entries = Object.entries(value).filter(([, val]) => val !== undefined && val !== null && val !== "");
  if (!entries.length) return "No details.";
  return entries.slice(0, 24).map(([key, val]) => {
    if (val && typeof val === "object") return `${indent}${humanizeKey(key)}:\n${readableValue(val, depth + 1)}`;
    return `${indent}${humanizeKey(key)}: ${formatPrimitive(val)}`;
  }).join("\n");
}

function readableInline(value) {
  if (value == null || typeof value !== "object") return formatPrimitive(value);
  if (Array.isArray(value)) return `${value.length} item${value.length === 1 ? "" : "s"}`;
  const preferred = [value.title, value.name, value.url, value.source_url, value.id].filter(Boolean).join(" ");
  if (preferred) return preferred;
  return Object.entries(value)
    .filter(([, val]) => val == null || typeof val !== "object")
    .slice(0, 4)
    .map(([key, val]) => `${humanizeKey(key)} ${formatPrimitive(val)}`)
    .join(", ") || truncate(formatJson(value), 180);
}

function badge(tone, label) {
  return `[[${tone || "neutral"}:${String(label || "").replace(/\]/g, "")}]]`;
}

function statLine(label, value, tone = "neutral") {
  return value == null ? "" : `${label}: ${badge(tone, Number(value).toLocaleString())}`;
}

function numberLine(label, value) {
  return value == null ? "" : `${label}: ${Number(value).toLocaleString()}`;
}

function toneForStatus(status) {
  const normalized = String(status || "").toLowerCase();
  if (["completed", "complete", "ok", "ready", "success", "succeeded"].includes(normalized)) return "success";
  if (["failed", "error", "blocked"].includes(normalized)) return "error";
  if (["canceled", "cancelled", "paused", "warn", "warning"].includes(normalized)) return "warn";
  if (["running", "pending", "queued", "active"].includes(normalized)) return "info";
  return "neutral";
}

function scoreTone(score) {
  const value = Number(score);
  if (!Number.isFinite(value)) return "neutral";
  if (value >= 0.75) return "success";
  if (value >= 0.45) return "info";
  if (value > 0) return "warn";
  return "neutral";
}

function toneForVerdict(verdict) {
  const normalized = String(verdict || "").toLowerCase();
  if (normalized.includes("pass") || normalized.includes("good") || normalized.includes("correct")) return "success";
  if (normalized.includes("fail") || normalized.includes("incorrect")) return "error";
  if (normalized.includes("partial") || normalized.includes("mixed")) return "warn";
  return "info";
}

function formatPrimitive(value) {
  if (value == null) return "none";
  if (typeof value === "boolean") return value ? "yes" : "no";
  if (typeof value === "number") return Number.isFinite(value) ? value.toLocaleString() : String(value);
  if (typeof value === "object") return truncate(formatJson(value), 180);
  return String(value);
}

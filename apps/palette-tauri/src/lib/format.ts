const SUMMARY_LIMIT = 10;

export type OutputKind = "markdown" | "code";

export function outputKindFor(subcommand: string): OutputKind {
  switch (subcommand) {
    case "ask":
    case "chat":
    case "scrape":
    case "summarize":
    case "research":
    case "suggest":
    case "endpoints":
    case "brand":
    case "diff":
    case "screenshot":
      return "markdown";
    default:
      return "code";
  }
}

export function formatPayload(subcommand: string, payload: unknown): string {
  if (typeof payload === "string") {
    return payload;
  }
  if (!isRecord(payload)) {
    return compact(payload);
  }
  const value = payload;
  switch (subcommand) {
    case "ask":
    case "chat":
      return stringField(value, "answer") ?? compact(value);
    case "scrape":
      return stringField(value, "markdown") ?? stringField(value, "output") ?? compact(value);
    case "retrieve":
      return stringField(value, "content") ?? compact(value);
    case "map":
      return stringArray(value, "urls")?.slice(0, 100).join("\n") || "No URLs discovered.";
    case "query":
      return resultRows(value, "results", (hit, index) => {
        const rank = numberField(hit, "rank") ?? index + 1;
        const score = numberField(hit, "score")?.toFixed(3) ?? "?";
        return `${rank}. score ${score}\n${stringField(hit, "url") ?? ""}\n${stringField(hit, "snippet") ?? ""}`.trim();
      });
    case "search":
    case "research":
      return formatSearchLike(value);
    case "summarize":
      return stringField(value, "summary") ?? compact(value);
    case "suggest":
      return resultRows(value, "suggestions", (suggestion) => {
        return `${stringField(suggestion, "url") ?? ""}\n${stringField(suggestion, "reason") ?? ""}`.trim();
      });
    case "evaluate":
      return (
        ["query", "analysis_answer", "rag_answer", "baseline_answer"]
          .map((key) => stringField(value, key))
          .filter(Boolean)
          .join("\n\n") || compact(value)
      );
    case "crawl":
    case "embed":
    case "extract":
    case "ingest":
    case "ingest-sessions-prepared":
      return jobStart(subcommand, value);
    case "endpoints":
      return endpointsReport(value);
    case "brand":
      return brandReport(value);
    case "diff":
      return diffReport(value);
    case "screenshot":
      return screenshotReport(value);
    case "dedupe":
      return dedupeReport(value);
    case "watch-list":
      return watchList(value);
    case "watch-create":
      return watchDefinition(value);
    case "watch-run":
      return watchRun(value);
    case "sources":
      return sourceList(value);
    case "domains":
      return resultRows(value, "domains", (domain) => compact(domain));
    case "doctor":
    case "stats":
    case "status":
    default:
      if (isJobLifecycle(subcommand)) return jobLifecycle(value);
      return compact(value);
  }
}

function formatSearchLike(value: Record<string, unknown>): string {
  const body = recordField(value, "payload") ?? value;
  return (
    stringField(body, "summary") ??
    optionalResultRows(body, "results", renderSearchResult) ??
    optionalResultRows(body, "search_results", renderSearchResult) ??
    compact(value)
  );
}

function renderSearchResult(hit: Record<string, unknown>, index: number): string {
  const title = stringField(hit, "title") ?? stringField(hit, "name") ?? "Untitled";
  const url = stringField(hit, "url") ?? "";
  const snippet = stringField(hit, "snippet") ?? stringField(hit, "content") ?? "";
  return `${index + 1}. ${title}\n${url}\n${snippet}`.trim();
}

function sourceList(value: Record<string, unknown>): string {
  const count = numberField(value, "count") ?? 0;
  const urls = stringArray(value, "urls") ?? [];
  return [`${count} indexed sources`, ...urls.slice(0, SUMMARY_LIMIT)].join("\n");
}

function endpointsReport(value: Record<string, unknown>): string {
  const urls =
    stringArray(value, "endpoints") ??
    stringArray(value, "urls") ??
    arrayField(value, "candidates")?.map((item) => (isRecord(item) ? stringField(item, "url") : undefined)).filter((item): item is string => Boolean(item)) ??
    [];
  const title = numberField(value, "total") ?? urls.length;
  return [`## Endpoint discovery`, `${title} candidates`, "", ...urls.slice(0, SUMMARY_LIMIT * 2)].join("\n");
}

function brandReport(value: Record<string, unknown>): string {
  const colors = arrayField(value, "colors") ?? [];
  const fonts = stringArray(value, "fonts") ?? [];
  const logos = arrayField(value, "logos") ?? [];
  const colorLines = colors.slice(0, SUMMARY_LIMIT).map((item) => {
    if (!isRecord(item)) return compact(item);
    const hex = stringField(item, "hex") ?? "";
    const usage = stringField(item, "usage") ?? "unknown";
    const count = numberField(item, "count");
    return `${hex} ${usage}${count === undefined ? "" : ` (${count})`}`.trim();
  });
  const logoLines = logos.slice(0, 6).map((item) => {
    if (!isRecord(item)) return compact(item);
    return `${stringField(item, "kind") ?? "logo"}: ${stringField(item, "url") ?? ""}`.trim();
  });
  return [
    `## ${stringField(value, "name") ?? "Brand"}`,
    stringField(value, "url") ?? "",
    "",
    colorLines.length ? "### Colors" : "",
    ...colorLines,
    fonts.length ? "### Fonts" : "",
    ...fonts.slice(0, SUMMARY_LIMIT),
    logoLines.length ? "### Assets" : "",
    stringField(value, "logo_url") ? `logo: ${stringField(value, "logo_url")}` : "",
    stringField(value, "favicon_url") ? `favicon: ${stringField(value, "favicon_url")}` : "",
    stringField(value, "og_image") ? `og image: ${stringField(value, "og_image")}` : "",
    ...logoLines,
  ]
    .filter(Boolean)
    .join("\n");
}

function diffReport(value: Record<string, unknown>): string {
  const metadata = arrayField(value, "metadata_changes") ?? [];
  const added = arrayField(value, "links_added") ?? [];
  const removed = arrayField(value, "links_removed") ?? [];
  const textDiff = stringField(value, "text_diff");
  const metaLines = metadata.slice(0, SUMMARY_LIMIT).map((item) => {
    if (!isRecord(item)) return compact(item);
    const field = stringField(item, "field") ?? "field";
    const oldValue = stringField(item, "old") ?? "";
    const newValue = stringField(item, "new") ?? "";
    return `${field}: ${oldValue} -> ${newValue}`;
  });
  return [
    `## Diff ${stringField(value, "status") ?? "unknown"}`,
    `${stringField(value, "url_a") ?? ""}`,
    `${stringField(value, "url_b") ?? ""}`,
    `word delta: ${numberField(value, "word_count_delta") ?? 0}`,
    `metadata changes: ${metadata.length}`,
    `links added: ${added.length}`,
    `links removed: ${removed.length}`,
    metaLines.length ? "### Metadata" : "",
    ...metaLines,
    textDiff ? "### Text diff" : "",
    textDiff ?? "",
  ]
    .filter(Boolean)
    .join("\n");
}

function screenshotReport(value: Record<string, unknown>): string {
  const artifact = recordField(value, "artifact_handle");
  return [
    "## Screenshot captured",
    stringField(value, "url") ?? "",
    stringField(value, "path") ? `path: ${stringField(value, "path")}` : "",
    artifact && stringField(artifact, "display_path") ? `display: ${stringField(artifact, "display_path")}` : "",
    numberField(value, "size_bytes") !== undefined ? `bytes: ${numberField(value, "size_bytes")}` : "",
  ]
    .filter(Boolean)
    .join("\n");
}

function dedupeReport(value: Record<string, unknown>): string {
  const pairs = [
    ["collection", stringField(value, "collection")],
    ["removed", numberField(value, "removed") ?? numberField(value, "points_deleted")],
    ["scanned", numberField(value, "scanned") ?? numberField(value, "points_scanned")],
  ].filter(([, field]) => field !== undefined);
  return pairs.map(([key, field]) => `${key}: ${field}`).join("\n") || compact(value);
}

function watchList(value: Record<string, unknown>): string {
  return resultRows(value, "watches", (watch) => watchDefinition(watch));
}

function watchDefinition(value: Record<string, unknown>): string {
  const id = stringField(value, "id") ?? stringField(value, "watch_id") ?? "";
  const name = stringField(value, "name") ?? "watch";
  const enabled = booleanField(value, "enabled");
  const every = numberField(value, "every_seconds");
  const next = stringField(value, "next_run_at");
  return [
    `${name}${id ? ` (${id})` : ""}`,
    enabled === undefined ? "" : `enabled: ${enabled}`,
    every === undefined ? "" : `every: ${every}s`,
    next ? `next: ${next}` : "",
  ]
    .filter(Boolean)
    .join("\n");
}

function watchRun(value: Record<string, unknown>): string {
  const artifacts = arrayField(value, "artifacts") ?? [];
  return [
    stringField(value, "watch_id") ? `watch: ${stringField(value, "watch_id")}` : "",
    stringField(value, "started_at") ? `started: ${stringField(value, "started_at")}` : "",
    stringField(value, "finished_at") ? `finished: ${stringField(value, "finished_at")}` : "",
    `artifacts: ${artifacts.length}`,
  ]
    .filter(Boolean)
    .join("\n") || compact(value);
}

function jobStart(subcommand: string, value: Record<string, unknown>): string {
  const result = recordField(value, "result") ?? value;
  const lines = [
    stringField(value, "disposition") ? `${subcommand} ${stringField(value, "disposition")}` : "",
    stringField(result, "status") ? `status: ${stringField(result, "status")}` : "",
    stringField(value, "execution_mode") ? `mode: ${stringField(value, "execution_mode")}` : "",
    stringField(result, "job_id") ? `job: ${stringField(result, "job_id")}` : "",
    stringField(value, "status_url") ? `status: ${stringField(value, "status_url")}` : "",
  ].filter(Boolean);
  return [...lines, "Next: status"].join("\n") || compact(value);
}

function jobLifecycle(value: Record<string, unknown>): string {
  const jobs = arrayField(value, "jobs") ?? arrayField(value, "items");
  if (jobs?.length) {
    return jobs
      .slice(0, SUMMARY_LIMIT)
      .map((job) => (isRecord(job) ? jobRow(job) : compact(job)))
      .join("\n\n");
  }
  return jobRow(value) || compact(value);
}

function jobRow(value: Record<string, unknown>): string {
  const id = stringField(value, "job_id") ?? stringField(value, "id") ?? "";
  const status = stringField(value, "status") ?? stringField(value, "state") ?? "";
  const kind = stringField(value, "kind") ?? stringField(value, "task_type") ?? "";
  const target = stringField(value, "target") ?? stringField(value, "url") ?? "";
  return [id, status ? `status: ${status}` : "", kind ? `kind: ${kind}` : "", target ? `target: ${target}` : ""]
    .filter(Boolean)
    .join("\n");
}

function isJobLifecycle(subcommand: string): boolean {
  return /^(crawl|embed|extract|ingest)-(list|status|cancel|cleanup|clear|recover)$/.test(subcommand);
}

function resultRows(
  value: Record<string, unknown>,
  key: string,
  render: (value: Record<string, unknown>, index: number) => string,
): string {
  return optionalResultRows(value, key, render) ?? compact(value);
}

function optionalResultRows(
  value: Record<string, unknown>,
  key: string,
  render: (value: Record<string, unknown>, index: number) => string,
): string | undefined {
  const rows = arrayField(value, key);
  if (!rows?.length) return undefined;
  return rows
    .slice(0, SUMMARY_LIMIT)
    .map((row, index) => (isRecord(row) ? render(row, index) : compact(row)))
    .join("\n\n");
}

function stringField(value: Record<string, unknown>, key: string): string | undefined {
  const field = value[key];
  return typeof field === "string" ? field : undefined;
}

function numberField(value: Record<string, unknown>, key: string): number | undefined {
  const field = value[key];
  return typeof field === "number" ? field : undefined;
}

function booleanField(value: Record<string, unknown>, key: string): boolean | undefined {
  const field = value[key];
  return typeof field === "boolean" ? field : undefined;
}

function recordField(value: Record<string, unknown>, key: string): Record<string, unknown> | undefined {
  const field = value[key];
  return isRecord(field) ? field : undefined;
}

function arrayField(value: Record<string, unknown>, key: string): unknown[] | undefined {
  const field = value[key];
  return Array.isArray(field) ? field : undefined;
}

function stringArray(value: Record<string, unknown>, key: string): string[] | undefined {
  return arrayField(value, key)?.filter((item): item is string => typeof item === "string");
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function compact(value: unknown): string {
  return JSON.stringify(value, null, 2) ?? String(value);
}

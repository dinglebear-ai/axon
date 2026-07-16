import {
  arrField as arrayField,
  boolField as booleanField,
  isRecord,
  numField as numberField,
  strField as stringField,
} from "./payload";

// Per-action text formatters (the fallback / copy text shown when a structured
// view is not used, and the body the OutputPanel renders for markdown/code
// runs). These pure functions are bound per subcommand in `actionRegistry.ts`
// (`ActionBehavior.formatText`); `format.ts` exposes the public `formatPayload`
// shim that dispatches through the registry. No JSX lives here — keep it pure so
// it can be imported by the lib layer.

const SUMMARY_LIMIT = 10;
const MAX_TEXT_DIFF_CHARS = 12_000;

/** Wrap a record-only formatter so it handles strings / non-records uniformly. */
export type RecordFormatter = (value: Record<string, unknown>) => string;

export function recordFormatter(fn: RecordFormatter): (payload: unknown) => string {
  return (payload: unknown) => {
    if (typeof payload === "string") return payload;
    if (!isRecord(payload)) return compact(payload);
    return fn(payload);
  };
}

export function formatAnswer(value: Record<string, unknown>): string {
  return stringField(value, "answer") ?? compact(value);
}

export function formatScrape(value: Record<string, unknown>): string {
  return stringField(value, "markdown") ?? stringField(value, "output") ?? compact(value);
}

export function formatRetrieve(value: Record<string, unknown>): string {
  return stringField(value, "content") ?? compact(value);
}

export function formatMap(value: Record<string, unknown>): string {
  return stringArray(value, "urls")?.slice(0, 100).join("\n") || "No URLs discovered.";
}

export function formatQuery(value: Record<string, unknown>): string {
  return resultRows(value, "results", (hit, index) => {
    const rank = numberField(hit, "rank") ?? index + 1;
    const score = numberField(hit, "score")?.toFixed(3) ?? "?";
    return `${rank}. score ${score}\n${stringField(hit, "url") ?? ""}\n${stringField(hit, "snippet") ?? ""}`.trim();
  });
}

export function formatSearchLike(value: Record<string, unknown>): string {
  const body = recordField(value, "payload") ?? value;
  return (
    stringField(body, "summary") ??
    optionalResultRows(body, "results", renderSearchResult) ??
    optionalResultRows(body, "search_results", renderSearchResult) ??
    compact(value)
  );
}

export function formatSummarize(value: Record<string, unknown>): string {
  return stringField(value, "summary") ?? compact(value);
}

export function formatSuggest(value: Record<string, unknown>): string {
  return resultRows(value, "suggestions", (suggestion) => {
    return `${stringField(suggestion, "url") ?? ""}\n${stringField(suggestion, "reason") ?? ""}`.trim();
  });
}

export function formatEvaluate(value: Record<string, unknown>): string {
  return (
    ["query", "analysis_answer", "rag_answer", "baseline_answer"]
      .map((key) => stringField(value, key))
      .filter(Boolean)
      .join("\n\n") || compact(value)
  );
}

export function jobStartFormatter(subcommand: string): RecordFormatter {
  return (value) => jobStart(subcommand, value);
}

export function formatEndpoints(value: Record<string, unknown>): string {
  const urls =
    stringArray(value, "endpoints") ??
    stringArray(value, "urls") ??
    arrayField(value, "candidates")
      .map((item) => (isRecord(item) ? stringField(item, "url") : undefined))
      .filter((item): item is string => Boolean(item)) ??
    [];
  const title = numberField(value, "total") ?? urls.length;
  return [`## Endpoint discovery`, `${title} candidates`, "", ...urls.slice(0, SUMMARY_LIMIT * 2)].join("\n");
}

export function formatBrand(value: Record<string, unknown>): string {
  const colors = arrayField(value, "colors");
  const fonts = stringArray(value, "fonts") ?? [];
  const logos = arrayField(value, "logos");
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

export function formatDiff(value: Record<string, unknown>): string {
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
    textDiff ? truncateTextDiff(textDiff) : "",
  ]
    .filter(Boolean)
    .join("\n");
}

export function formatScreenshot(value: Record<string, unknown>): string {
  const artifact = recordField(value, "artifact_handle");
  const artifactDisplay = artifact
    ? (nonEmptyStringField(artifact, "display_path") ?? nonEmptyStringField(artifact, "relative_path"))
    : undefined;
  return [
    "## Screenshot captured",
    stringField(value, "url") ?? "",
    artifactDisplay ? `artifact: ${artifactDisplay}` : "",
    numberField(value, "size_bytes") !== undefined ? `bytes: ${numberField(value, "size_bytes")}` : "",
  ]
    .filter(Boolean)
    .join("\n");
}

function nonEmptyStringField(value: Record<string, unknown>, key: string): string | undefined {
  const text = stringField(value, key)?.trim();
  return text ? text : undefined;
}

export function formatWatchList(value: Record<string, unknown>): string {
  return optionalResultRows(value, "items", (watch) => watchDefinition(watch))
    ?? resultRows(value, "watches", (watch) => watchDefinition(watch));
}

export function formatWatchCreate(value: Record<string, unknown>): string {
  return watchDefinition(value);
}

export function formatWatchRun(value: Record<string, unknown>): string {
  const artifacts = arrayField(value, "artifacts") ?? [];
  return (
    [
      stringField(value, "watch_id") ? `watch: ${stringField(value, "watch_id")}` : "",
      stringField(value, "started_at") ? `started: ${stringField(value, "started_at")}` : "",
      stringField(value, "finished_at") ? `finished: ${stringField(value, "finished_at")}` : "",
      `artifacts: ${artifacts.length}`,
    ]
      .filter(Boolean)
      .join("\n") || compact(value)
  );
}

export function formatSources(value: Record<string, unknown>): string {
  const count = numberField(value, "count") ?? 0;
  const urls = stringArray(value, "urls") ?? [];
  return [`${count} indexed sources`, ...urls.slice(0, SUMMARY_LIMIT)].join("\n");
}

export function formatDomains(value: Record<string, unknown>): string {
  return resultRows(value, "domains", (domain) => compact(domain));
}

export function formatJobLifecycle(value: Record<string, unknown>): string {
  return jobLifecycle(value);
}

/** Fallback text for the `github` browse result — the structured view
 * (`GitHubView`) is the primary UI; this is the copy/plaintext fallback. */
export function formatGitHub(value: Record<string, unknown>): string {
  if (booleanField(value, "ok") === false) {
    return stringField(value, "error") ?? "GitHub request failed.";
  }
  const kind = stringField(value, "kind") ?? "repos";
  const payload = value.payload;
  if (kind === "repos" && Array.isArray(payload)) {
    return (
      payload
        .map((repo) => (isRecord(repo) ? stringField(repo, "full_name") ?? stringField(repo, "name") : null))
        .filter((name): name is string => Boolean(name))
        .join("\n") || "No repositories found."
    );
  }
  if (kind === "file" && isRecord(payload)) {
    return stringField(payload, "path") ?? "File preview.";
  }
  return compact(payload);
}

/** Fallback formatter for actions with no structured text shape (status, etc.). */
export function formatCompact(payload: unknown): string {
  if (typeof payload === "string") return payload;
  return compact(payload);
}

function renderSearchResult(hit: Record<string, unknown>, index: number): string {
  const title = stringField(hit, "title") ?? stringField(hit, "name") ?? "Untitled";
  const url = stringField(hit, "url") ?? "";
  const snippet = stringField(hit, "snippet") ?? stringField(hit, "content") ?? "";
  return `${index + 1}. ${title}\n${url}\n${snippet}`.trim();
}

function truncateTextDiff(text: string): string {
  if (text.length <= MAX_TEXT_DIFF_CHARS) return text;
  const omitted = text.length - MAX_TEXT_DIFF_CHARS;
  return `${text.slice(0, MAX_TEXT_DIFF_CHARS)}\n\n[truncated ${omitted} chars from text_diff]`;
}

function watchDefinition(value: Record<string, unknown>): string {
  const id = stringField(value, "id") ?? stringField(value, "watch_id") ?? "";
  const name = stringField(value, "name") ?? stringField(value, "source_id") ?? "watch";
  const enabled = booleanField(value, "enabled");
  const schedule = recordField(value, "schedule");
  const every = numberField(value, "every_seconds") ?? (schedule ? numberField(schedule, "every_seconds") : undefined);
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

function recordField(value: Record<string, unknown>, key: string): Record<string, unknown> | undefined {
  const field = value[key];
  return isRecord(field) ? field : undefined;
}

function stringArray(value: Record<string, unknown>, key: string): string[] | undefined {
  const strings = arrayField(value, key).filter((item): item is string => typeof item === "string");
  return strings.length ? strings : undefined;
}

function compact(value: unknown): string {
  return JSON.stringify(value, null, 2) ?? String(value);
}

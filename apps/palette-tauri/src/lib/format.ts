const SUMMARY_LIMIT = 10;

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
      return jobStart(subcommand, value);
    case "sources":
      return sourceList(value);
    case "domains":
      return resultRows(value, "domains", (domain) => compact(domain));
    case "doctor":
    case "stats":
    case "status":
    default:
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

function jobStart(subcommand: string, value: Record<string, unknown>): string {
  const result = recordField(value, "result") ?? value;
  const lines = [
    stringField(value, "disposition") ? `${subcommand} ${stringField(value, "disposition")}` : "",
    stringField(value, "execution_mode") ? `mode: ${stringField(value, "execution_mode")}` : "",
    stringField(result, "job_id") ? `job: ${stringField(result, "job_id")}` : "",
  ].filter(Boolean);
  return [...lines, "Next: status"].join("\n") || compact(value);
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

import {
  ACTIONS,
  type PaletteAction,
  acceptsDirectUrl,
  actionInvokedBy,
} from "@/lib/actions";
import type { PaletteResult } from "@/lib/axonClient";

export type ParsedCommand = { invoked?: PaletteAction; search: string; arg: string };
export type RunState =
  | { kind: "idle" }
  | { kind: "running"; title: string; subtitle: string }
  | { kind: "queued" | "success" | "error"; title: string; subtitle: string; text: string; result: PaletteResult };

export function focusInput(select = false) {
  window.setTimeout(() => {
    const input = document.querySelector<HTMLInputElement>(".command-input");
    input?.focus();
    if (select) input?.select();
  }, 30);
}

export function parseCommand(raw: string): ParsedCommand {
  const trimmed = raw.trimStart();
  const [token = ""] = trimmed.split(/\s+/);
  const invoked = ACTIONS.find((action) => actionInvokedBy(action, token));
  if (invoked) {
    return { invoked, search: token, arg: trimmed.slice(token.length).trimStart() };
  }
  return { search: trimmed, arg: "" };
}

export function argumentFor(
  action: PaletteAction,
  modeAction: PaletteAction | null,
  parsed: ParsedCommand,
  query: string,
): string {
  if (modeAction?.subcommand === action.subcommand) return query.trim();
  if (parsed.invoked?.subcommand === action.subcommand) return parsed.arg;
  if (looksLikeUrl(parsed.search) && acceptsDirectUrl(action)) return parsed.search;
  return parsed.search;
}

export function validationMessage(action: PaletteAction, argument: string): string {
  if (action.argMode === "none" || action.argMode === "optionalSingle") return "";
  return argument.trim() ? "" : "Argument required";
}

export function actionHint(action: PaletteAction, search: string): string {
  if (acceptsDirectUrl(action) && looksLikeUrl(search)) return "Run URL";
  if (action.argMode === "none") return "Run";
  return "Select";
}

export function actionKindLabel(action: PaletteAction): string {
  switch (action.kind) {
    case "admin":
      return "Admin";
    case "discovery":
      return "Discovery";
    case "job":
      return "Job";
    case "operation":
    default:
      return "Operation";
  }
}

export function actionArgumentLabel(action: PaletteAction): string {
  switch (action.argMode) {
    case "none":
      return "No input";
    case "optionalSingle":
      return "Optional input";
    case "single":
      return "Text input";
    case "split":
      return "Structured input";
  }
}

export function actionKindTone(action: PaletteAction): "info" | "success" | "warn" | "neutral" | "rose" | "violet" {
  switch (action.kind) {
    case "admin":
      return "warn";
    case "discovery":
      return "neutral";
    case "job":
      return "violet";
    case "operation":
    default:
      return "info";
  }
}

export type ActionDisplayMeta = {
  category: string;
  endpoint: string;
  input: string;
  output: string;
  label: string;
  method: "GET" | "POST";
};

const DISPLAY_META: Record<string, ActionDisplayMeta> = {
  scrape: {
    category: "Fetch & read",
    endpoint: "/v1/scrape",
    input: "one URL",
    output: "content",
    label: "Scrape",
    method: "POST",
  },
  map: {
    category: "Fetch & read",
    endpoint: "/v1/map",
    input: "one URL",
    output: "links",
    label: "Map",
    method: "POST",
  },
  retrieve: {
    category: "Fetch & read",
    endpoint: "/v1/retrieve",
    input: "URL",
    output: "chunks",
    label: "Retrieve",
    method: "GET",
  },
  screenshot: {
    category: "Fetch & read",
    endpoint: "/v1/screenshot",
    input: "URL",
    output: "PNG",
    label: "Screenshot",
    method: "POST",
  },
  diff: {
    category: "Fetch & read",
    endpoint: "/v1/diff",
    input: "two URLs",
    output: "changes",
    label: "Diff",
    method: "POST",
  },
  crawl: {
    category: "Crawl & ingest",
    endpoint: "/v1/crawl",
    input: "start URL",
    output: "job",
    label: "Crawl",
    method: "POST",
  },
  ingest: {
    category: "Crawl & ingest",
    endpoint: "/v1/ingest",
    input: "target",
    output: "job",
    label: "Ingest",
    method: "POST",
  },
  embed: {
    category: "Crawl & ingest",
    endpoint: "/v1/embed",
    input: "input",
    output: "vectors",
    label: "Embed",
    method: "POST",
  },
  extract: {
    category: "Crawl & ingest",
    endpoint: "/v1/extract",
    input: "URLs",
    output: "data",
    label: "Extract",
    method: "POST",
  },
  search: {
    category: "Search & discover",
    endpoint: "/v1/search",
    input: "query",
    output: "results",
    label: "Search",
    method: "POST",
  },
  research: {
    category: "Search & discover",
    endpoint: "/v1/research",
    input: "query",
    output: "brief",
    label: "Research",
    method: "POST",
  },
  query: {
    category: "Search & discover",
    endpoint: "/v1/query",
    input: "query",
    output: "chunks",
    label: "Query",
    method: "POST",
  },
  sources: {
    category: "Search & discover",
    endpoint: "/v1/sources",
    input: "none",
    output: "URLs",
    label: "Sources",
    method: "GET",
  },
  domains: {
    category: "Search & discover",
    endpoint: "/v1/domains",
    input: "none",
    output: "domains",
    label: "Domains",
    method: "GET",
  },
  ask: {
    category: "Reason",
    endpoint: "/v1/ask",
    input: "question",
    output: "answer",
    label: "Ask",
    method: "POST",
  },
  summarize: {
    category: "Reason",
    endpoint: "/v1/summarize",
    input: "URLs",
    output: "summary",
    label: "Summarize",
    method: "POST",
  },
  suggest: {
    category: "Reason",
    endpoint: "/v1/suggest",
    input: "focus",
    output: "URLs",
    label: "Suggest",
    method: "POST",
  },
  evaluate: {
    category: "Reason",
    endpoint: "/v1/evaluate",
    input: "question",
    output: "score",
    label: "Evaluate",
    method: "POST",
  },
  status: {
    category: "System",
    endpoint: "/v1/status",
    input: "none",
    output: "jobs",
    label: "Status",
    method: "GET",
  },
  stats: {
    category: "System",
    endpoint: "/v1/stats",
    input: "none",
    output: "stats",
    label: "Stats",
    method: "GET",
  },
  doctor: {
    category: "System",
    endpoint: "/v1/doctor",
    input: "none",
    output: "health",
    label: "Doctor",
    method: "GET",
  },
  brand: {
    category: "System",
    endpoint: "/v1/brand",
    input: "URL",
    output: "brand",
    label: "Brand",
    method: "POST",
  },
  endpoints: {
    category: "System",
    endpoint: "/v1/endpoints",
    input: "URL",
    output: "routes",
    label: "Endpoints",
    method: "POST",
  },
  dedupe: {
    category: "System",
    endpoint: "/v1/dedupe",
    input: "collection",
    output: "report",
    label: "Dedupe",
    method: "POST",
  },
};

export function actionDisplayMeta(action: PaletteAction): ActionDisplayMeta {
  return (
    DISPLAY_META[action.subcommand] ?? {
      category: action.kind === "admin" || action.kind === "discovery" ? "System" : "Actions",
      endpoint: `/v1/${action.subcommand.replace(/-/g, "/")}`,
      input: action.argMode === "none" ? "none" : "input",
      output: "result",
      label: action.label.replace(/\s+(URL|input|question|target|document|collection)$/i, ""),
      method: action.argMode === "none" ? "GET" : "POST",
    }
  );
}

const CATEGORY_ORDER = ["Fetch & read", "Crawl & ingest", "Search & discover", "Reason", "System", "Actions"];
const ACTION_ORDER = [
  "scrape",
  "map",
  "retrieve",
  "screenshot",
  "diff",
  "crawl",
  "ingest",
  "embed",
  "extract",
  "search",
  "research",
  "query",
  "sources",
  "domains",
  "ask",
  "summarize",
  "suggest",
  "evaluate",
  "status",
  "stats",
  "doctor",
  "brand",
  "endpoints",
  "dedupe",
];

export function sortActionsForDisplay(actions: PaletteAction[]): PaletteAction[] {
  return [...actions].sort((a, b) => {
    const metaA = actionDisplayMeta(a);
    const metaB = actionDisplayMeta(b);
    const categoryDelta = rank(CATEGORY_ORDER, metaA.category) - rank(CATEGORY_ORDER, metaB.category);
    if (categoryDelta) return categoryDelta;
    return rank(ACTION_ORDER, a.subcommand) - rank(ACTION_ORDER, b.subcommand);
  });
}

function rank(order: string[], value: string): number {
  const index = order.indexOf(value);
  return index === -1 ? order.length : index;
}

export function argumentPlaceholder(action: PaletteAction): string {
  const example = action.example.replace(new RegExp(`^${action.subcommand}\\s*`, "i"), "").trim();
  return example || action.description;
}

export function looksLikeUrl(value: string): boolean {
  const trimmed = value.trim();
  return /^https?:\/\//i.test(trimmed) || /^[a-z0-9-]+(\.[a-z0-9-]+)+(\/\S*)?$/i.test(trimmed);
}

export function hostLabel(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

export function firstUrl(value: string): string | null {
  return value.match(/https?:\/\/[^\s"')\]}]+/i)?.[0] ?? null;
}

export function runTone(run: RunState): "info" | "success" | "error" | "neutral" {
  if (run.kind === "success") return "success";
  if (run.kind === "error") return "error";
  if (run.kind === "running" || run.kind === "queued") return "info";
  return "neutral";
}

export function outputTitle(run: RunState): string {
  if (run.kind === "idle") return "Ready";
  return run.title;
}

export function outputSubtitle(run: RunState, action: PaletteAction | undefined): string {
  if (run.kind === "idle") return action?.description ?? "No matching action";
  return run.subtitle;
}

export function asyncJobStart(payload: unknown): { jobId: string; status: string } | null {
  const result = recordField(payload, "result") ?? recordField(payload, "job") ?? payload;
  if (!isRecord(result)) return null;
  const jobId = stringField(result, "job_id") ?? stringField(result, "id");
  if (!jobId) return null;
  const rawStatus =
    stringField(result, "status") ??
    stringField(recordField(payload, "result") ?? {}, "status") ??
    stringField(payload, "disposition") ??
    "queued";
  if (/^(completed|failed|error)$/i.test(rawStatus)) return null;
  return { jobId, status: "queued" };
}

function stringField(value: unknown, key: string): string | undefined {
  return isRecord(value) && typeof value[key] === "string" ? value[key] : undefined;
}

function recordField(value: unknown, key: string): Record<string, unknown> | undefined {
  return isRecord(value) && isRecord(value[key]) ? value[key] : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

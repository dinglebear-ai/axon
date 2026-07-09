import type { PaletteAction, PaletteSubcommand } from "@/lib/actions";
import { actionRouteTemplate, type HttpMethod } from "@/lib/axonClient";

export type ActionDisplayMeta = {
  category: string;
  endpoint: string;
  input: string;
  output: string;
  label: string;
  method: HttpMethod;
};

type ActionDisplayDetails = Omit<ActionDisplayMeta, "endpoint" | "method">;

const ACTION_META: Partial<Record<PaletteSubcommand, ActionDisplayDetails>> = {
  help: { category: "System", input: "action", output: "help", label: "Help" },
  files: { category: "System", input: "none", output: "local", label: "Files" },
  scrape: { category: "Fetch & read", input: "one URL", output: "content", label: "Scrape" },
  map: { category: "Fetch & read", input: "one URL", output: "links", label: "Map" },
  retrieve: { category: "Fetch & read", input: "URL", output: "chunks", label: "Retrieve" },
  screenshot: { category: "Fetch & read", input: "URL", output: "PNG", label: "Screenshot" },
  diff: { category: "Fetch & read", input: "two URLs", output: "changes", label: "Diff" },
  crawl: { category: "Crawl & ingest", input: "start URL", output: "job", label: "Crawl" },
  ingest: { category: "Crawl & ingest", input: "target", output: "job", label: "Ingest" },
  embed: { category: "Crawl & ingest", input: "input", output: "vectors", label: "Embed" },
  extract: { category: "Crawl & ingest", input: "URLs", output: "data", label: "Extract" },
  "ingest-sessions-prepared": { category: "Crawl & ingest", input: "JSON", output: "job", label: "Prepared sessions" },
  search: { category: "Search & discover", input: "query", output: "results", label: "Search" },
  research: { category: "Search & discover", input: "query", output: "brief", label: "Research" },
  query: { category: "Search & discover", input: "query", output: "chunks", label: "Query" },
  sources: { category: "Search & discover", input: "none", output: "URLs", label: "Sources" },
  domains: { category: "Search & discover", input: "none", output: "domains", label: "Domains" },
  ask: { category: "Reason", input: "question", output: "answer", label: "Ask" },
  chat: { category: "Reason", input: "message", output: "answer", label: "Chat" },
  summarize: { category: "Reason", input: "URLs", output: "summary", label: "Summarize" },
  suggest: { category: "Reason", input: "focus", output: "URLs", label: "Suggest" },
  evaluate: { category: "Reason", input: "question", output: "score", label: "Evaluate" },
  status: { category: "System", input: "none", output: "jobs", label: "Status" },
  stats: { category: "System", input: "none", output: "stats", label: "Stats" },
  doctor: { category: "System", input: "none", output: "health", label: "Doctor" },
  endpoints: { category: "Fetch & read", input: "URL", output: "endpoints", label: "Endpoints" },
  brand: { category: "Fetch & read", input: "URL", output: "brand", label: "Brand" },
  dedupe: { category: "System", input: "settings", output: "report", label: "Dedupe" },
  purge: { category: "System", input: "url", output: "report", label: "Purge" },
  "watch-list": { category: "System", input: "none", output: "watches", label: "Watch list" },
  "watch-create": { category: "System", input: "URL", output: "watch", label: "Watch create" },
  "watch-run": { category: "System", input: "watch id", output: "run", label: "Watch run" },
};

export function actionDisplayMeta(action: PaletteAction): ActionDisplayMeta {
  const details =
    ACTION_META[action.subcommand] ??
    lifecycleDisplayDetails(action) ?? {
      category: action.kind === "local" ? "System" : "Other",
      input: action.argMode === "none" ? "none" : "input",
      output: action.kind === "local" ? "local" : "result",
      label: action.label,
    };
  const route =
    action.kind === "local"
      ? { method: "GET" as const, path: `palette://${action.subcommand}` }
      : actionRouteTemplate(action.subcommand);
  return { ...details, endpoint: route.path, method: route.method };
}

export function actionKindLabel(action: PaletteAction): string {
  switch (action.kind) {
    case "admin":
      return "Admin";
    case "discovery":
      return "Discovery";
    case "job":
      return "Job";
    case "local":
      return "Local";
    default:
      return "Operation";
  }
}

export function actionKindTone(action: PaletteAction): "info" | "success" | "warn" | "neutral" | "rose" | "orange" {
  switch (action.kind) {
    case "admin":
      return "warn";
    case "discovery":
      return "neutral";
    case "job":
      return "orange";
    case "local":
      return "info";
    default:
      return "info";
  }
}

function lifecycleDisplayDetails(action: PaletteAction): ActionDisplayDetails | undefined {
  const match = /^(crawl|embed|extract|ingest)-(list|status|cancel|cleanup|clear|recover)$/.exec(action.subcommand);
  if (!match) return undefined;
  const [, , operation] = match;
  const input = ["list", "cleanup", "clear", "recover"].includes(operation) ? "none" : "job id";
  return {
    category: "Jobs",
    input,
    output: operation === "list" ? "job list" : operation === "status" ? "job status" : `${operation} report`,
    label: action.label,
  };
}

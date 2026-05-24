import createClient from "openapi-fetch";
import { fetch as tauriFetch } from "@tauri-apps/plugin-http";

import type { components, paths } from "./axon-api";
import type { PaletteAction } from "./actions";
import { splitShellWords } from "./shellWords";

export interface PaletteConfig {
  serverUrl: string;
  token?: string | null;
}

export interface PaletteResult {
  ok: boolean;
  status: number;
  path: string;
  method: "GET" | "POST";
  payload: unknown;
}

type Client = ReturnType<typeof createClient<paths>>;

export function createAxonClient(config: PaletteConfig): Client {
  const headers = config.token ? { Authorization: `Bearer ${config.token}` } : undefined;
  return createClient<paths>({
    baseUrl: config.serverUrl.replace(/\/+$/, ""),
    headers,
    fetch: tauriFetch as unknown as typeof globalThis.fetch,
  });
}

export async function executeAction(
  client: Client,
  action: PaletteAction,
  arg: string,
): Promise<PaletteResult> {
  const words = wordsFor(action, arg);

  switch (action.subcommand) {
    case "doctor":
      return getResult(client, "/v1/doctor");
    case "status":
      return getResult(client, "/v1/status");
    case "sources":
      return getResult(client, "/v1/sources");
    case "domains":
      return getResult(client, "/v1/domains");
    case "stats":
      return getResult(client, "/v1/stats");
    case "scrape":
      return postResult(client, "/v1/scrape", { url: first(words, "url") });
    case "crawl":
      return postResult(client, "/v1/crawl", { urls: required(words, "urls") });
    case "map":
      return postResult(client, "/v1/map", { url: first(words, "url"), limit: 100 });
    case "summarize":
      return postResult(client, "/v1/summarize", { urls: required(words, "urls") });
    case "ask":
      return postResult(client, "/v1/ask", {
        query: first(words, "query"),
        explain: false,
        diagnostics: false,
      });
    case "query":
      return postResult(client, "/v1/query", { query: first(words, "query"), limit: 10 });
    case "retrieve":
      return postResult(client, "/v1/retrieve", {
        url: first(words, "url"),
        token_budget: 6000,
      });
    case "suggest":
      return postResult(client, "/v1/suggest", words[0] ? { focus: words[0] } : {});
    case "evaluate":
      return postResult(client, "/v1/evaluate", { question: first(words, "question") });
    case "search":
      return postResult(client, "/v1/search", { query: first(words, "query"), limit: 10 });
    case "research":
      return postResult(client, "/v1/research", { query: first(words, "query"), limit: 10 });
    case "embed":
      return postResult(client, "/v1/embed", { input: first(words, "input") });
    case "extract":
      return postResult(client, "/v1/extract", { urls: required(words, "urls") });
    case "ingest":
      return postResult(client, "/v1/ingest", ingestBody(first(words, "target")));
    default:
      throw new Error(`REST route is not wired for ${action.subcommand}`);
  }
}

function wordsFor(action: PaletteAction, arg: string): string[] {
  switch (action.argMode) {
    case "none":
      return [];
    case "optionalSingle":
      return arg.trim() ? [arg.trim()] : [];
    case "single":
      return [arg.trim()];
    case "split":
      return splitShellWords(arg);
  }
}

function first(words: string[], field: string): string {
  return required(words, field)[0];
}

function required(words: string[], field: string): string[] {
  const clean = words.map((word) => word.trim()).filter(Boolean);
  if (!clean.length) throw new Error(`${field} is required`);
  return clean;
}

function ingestBody(target: string): Record<string, unknown> {
  const lower = target.toLowerCase();
  if (lower.includes("youtube.com/") || lower.includes("youtu.be/")) {
    return { source_type: "youtube", target };
  }
  if (lower.includes("reddit.com/") || lower.startsWith("/r/") || lower.startsWith("r/")) {
    return { source_type: "reddit", target };
  }
  return { source_type: "github", repo: target, include_source: true };
}

type GetPath = "/v1/doctor" | "/v1/status" | "/v1/sources" | "/v1/domains" | "/v1/stats";
type PostPath =
  | "/v1/scrape"
  | "/v1/crawl"
  | "/v1/map"
  | "/v1/summarize"
  | "/v1/ask"
  | "/v1/query"
  | "/v1/retrieve"
  | "/v1/suggest"
  | "/v1/evaluate"
  | "/v1/search"
  | "/v1/research"
  | "/v1/embed"
  | "/v1/extract"
  | "/v1/ingest";

async function getResult<Path extends GetPath>(
  client: Client,
  path: Path,
): Promise<PaletteResult> {
  try {
    const { data, error, response } = await (client.GET as AnyGet)(path, {});
    return {
      ok: response.ok,
      status: response.status,
      path: String(path),
      method: "GET",
      payload: data ?? error ?? null,
    };
  } catch (error) {
    return failedResult("GET", path, error);
  }
}

async function postResult<Path extends PostPath>(
  client: Client,
  path: Path,
  body: components["schemas"][keyof components["schemas"]] | Record<string, unknown>,
): Promise<PaletteResult> {
  try {
    const { data, error, response } = await (client.POST as AnyPost)(path, { body });
    return {
      ok: response.ok,
      status: response.status,
      path: String(path),
      method: "POST",
      payload: data ?? error ?? null,
    };
  } catch (error) {
    return failedResult("POST", path, error);
  }
}

function failedResult(method: PaletteResult["method"], path: GetPath | PostPath, error: unknown): PaletteResult {
  return {
    ok: false,
    status: 0,
    path: String(path),
    method,
    payload: {
      error: error instanceof Error ? error.message : String(error),
    },
  };
}

type AnyGet = (path: GetPath, init?: Record<string, unknown>) => Promise<{
  data?: unknown;
  error?: unknown;
  response: Response;
}>;

type AnyPost = (
  path: PostPath,
  init: { body: Record<string, unknown> },
) => Promise<{
  data?: unknown;
  error?: unknown;
  response: Response;
}>;

import { invoke } from "@tauri-apps/api/core";

import type { PaletteAction } from "./actions";
import { splitShellWords } from "./shellWords";

export interface PaletteConfig {
  serverUrl: string;
  token?: string | null;
  shortcut: string;
  collection: string;
  resultLimit: number;
  theme: "system" | "dark" | "light";
  hideOnBlur: boolean;
}

export interface PaletteResult {
  ok: boolean;
  status: number;
  path: string;
  method: "GET" | "POST";
  payload: unknown;
}

interface Client {
  baseUrl: string;
  headers: Record<string, string>;
}

export function createAxonClient(config: PaletteConfig): Client {
  const token = config.token?.trim();
  return {
    baseUrl: normalizeServerUrl(config.serverUrl),
    headers: token ? { Authorization: `Bearer ${token}`, "x-api-key": token } : {},
  };
}

function normalizeServerUrl(value: string): string {
  const trimmed = value.trim().replace(/\/+$/, "");
  if (!trimmed || trimmed.includes("://")) return trimmed;
  if (trimmed.startsWith("localhost") || trimmed.startsWith("127.0.0.1")) {
    return `http://${trimmed}`;
  }
  return `https://${trimmed}`;
}

export async function executeAction(
  client: Client,
  action: PaletteAction,
  arg: string,
  config: PaletteConfig,
): Promise<PaletteResult> {
  const words = wordsFor(action, arg);
  const collection = config.collection.trim();
  const collectionBody = collection ? { collection } : {};
  const limit = config.resultLimit || 10;

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
      return postResult(client, "/v1/scrape", { url: first(words, "url"), ...collectionBody });
    case "crawl":
      return postResult(client, "/v1/crawl", { urls: required(words, "urls"), ...collectionBody });
    case "map":
      return postResult(client, "/v1/map", { url: first(words, "url"), limit: 100 });
    case "summarize":
      return postResult(client, "/v1/summarize", { urls: required(words, "urls"), ...collectionBody });
    case "ask":
      return postResult(client, "/v1/ask", {
        query: first(words, "query"),
        explain: false,
        diagnostics: false,
        ...collectionBody,
      });
    case "query":
      return postResult(client, "/v1/query", { query: first(words, "query"), limit, ...collectionBody });
    case "retrieve":
      return postResult(client, "/v1/retrieve", {
        url: first(words, "url"),
        token_budget: 6000,
        ...collectionBody,
      });
    case "suggest":
      return postResult(client, "/v1/suggest", words[0] ? { focus: words[0] } : {});
    case "evaluate":
      return postResult(client, "/v1/evaluate", { question: first(words, "question") });
    case "search":
      return postResult(client, "/v1/search", { query: first(words, "query"), limit });
    case "research":
      return postResult(client, "/v1/research", { query: first(words, "query"), limit });
    case "embed":
      return postResult(client, "/v1/embed", { input: first(words, "input"), ...collectionBody });
    case "extract":
      return postResult(client, "/v1/extract", { urls: required(words, "urls"), ...collectionBody });
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
    return await invoke<PaletteResult>("axon_http_request", {
      request: {
        baseUrl: client.baseUrl,
        token: tokenFromHeaders(client.headers),
        method: "GET",
        path,
        body: null,
      },
    });
  } catch (error) {
    return failedResult("GET", path, error);
  }
}

async function postResult<Path extends PostPath>(
  client: Client,
  path: Path,
  body: Record<string, unknown>,
): Promise<PaletteResult> {
  try {
    return await invoke<PaletteResult>("axon_http_request", {
      request: {
        baseUrl: client.baseUrl,
        token: tokenFromHeaders(client.headers),
        method: "POST",
        path,
        body,
      },
    });
  } catch (error) {
    return failedResult("POST", path, error);
  }
}

function tokenFromHeaders(headers: Record<string, string>): string | null {
  const authorization = headers.Authorization;
  if (authorization?.startsWith("Bearer ")) {
    return authorization.slice("Bearer ".length);
  }
  return headers["x-api-key"] ?? null;
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

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

export interface Client {
  baseUrl: string;
  headers: Record<string, string>;
}

export interface PaletteHttpRequest {
  baseUrl: string;
  token: string | null;
  method: "GET" | "POST";
  path: string;
  body: Record<string, unknown> | null;
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
  const request = buildActionRequest(client, action, arg, config);
  try {
    return await invoke<PaletteResult>("axon_http_request", { request });
  } catch (error) {
    return failedResult(request.method, request.path, error);
  }
}

export function buildActionRequest(
  client: Client,
  action: PaletteAction,
  arg: string,
  config: PaletteConfig,
): PaletteHttpRequest {
  const body = bodyFor(action, arg, config);
  return {
    baseUrl: client.baseUrl,
    token: tokenFromHeaders(client.headers),
    method: body.method,
    path: body.path,
    body: body.body,
  };
}

function bodyFor(
  action: PaletteAction,
  arg: string,
  config: PaletteConfig,
): { method: "GET" | "POST"; path: GetPath | PostPath; body: Record<string, unknown> | null } {
  const words = wordsFor(action, arg);
  const collection = config.collection.trim();
  const collectionBody = collection ? { collection } : {};
  const limit = config.resultLimit || 10;

  switch (action.subcommand) {
    case "doctor":
      return { method: "GET", path: "/v1/doctor", body: null };
    case "status":
      return { method: "GET", path: "/v1/status", body: null };
    case "sources":
      return { method: "GET", path: "/v1/sources", body: null };
    case "domains":
      return { method: "GET", path: "/v1/domains", body: null };
    case "stats":
      return { method: "GET", path: "/v1/stats", body: null };
    case "scrape":
      return { method: "POST", path: "/v1/scrape", body: { url: first(words, "url"), ...collectionBody } };
    case "crawl":
      return { method: "POST", path: "/v1/crawl", body: { urls: required(words, "urls"), ...collectionBody } };
    case "map":
      return { method: "POST", path: "/v1/map", body: { url: first(words, "url"), limit: 100 } };
    case "summarize":
      return { method: "POST", path: "/v1/summarize", body: { urls: required(words, "urls"), ...collectionBody } };
    case "ask":
      return {
        method: "POST",
        path: "/v1/ask",
        body: {
          query: first(words, "query"),
          explain: false,
          diagnostics: false,
          ...collectionBody,
        },
      };
    case "query":
      return { method: "POST", path: "/v1/query", body: { query: first(words, "query"), limit, ...collectionBody } };
    case "retrieve":
      return {
        method: "POST",
        path: "/v1/retrieve",
        body: {
          url: first(words, "url"),
          token_budget: 6000,
          ...collectionBody,
        },
      };
    case "suggest":
      return { method: "POST", path: "/v1/suggest", body: words[0] ? { focus: words[0] } : {} };
    case "evaluate":
      return { method: "POST", path: "/v1/evaluate", body: { question: first(words, "question") } };
    case "search":
      return { method: "POST", path: "/v1/search", body: { query: first(words, "query"), limit } };
    case "research":
      return { method: "POST", path: "/v1/research", body: { query: first(words, "query"), limit } };
    case "embed":
      return { method: "POST", path: "/v1/embed", body: { input: first(words, "input"), ...collectionBody } };
    case "extract":
      return { method: "POST", path: "/v1/extract", body: { urls: required(words, "urls"), ...collectionBody } };
    case "ingest":
      return { method: "POST", path: "/v1/ingest", body: ingestBody(first(words, "target")) };
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

function tokenFromHeaders(headers: Record<string, string>): string | null {
  const authorization = headers.Authorization;
  if (authorization?.startsWith("Bearer ")) {
    return authorization.slice("Bearer ".length);
  }
  return headers["x-api-key"] ?? null;
}

function failedResult(method: PaletteResult["method"], path: string, error: unknown): PaletteResult {
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

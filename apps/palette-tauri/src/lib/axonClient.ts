// NOTE (M1): OpenAPI types are generated into `./axon-api.d.ts` via
// `pnpm generate:api`.  Request execution is currently hand-coded here; a full
// migration to the generated request helpers (`openapi-fetch`) is tracked in
// GitHub issue #177 (finding M1).  Until that migration is complete the
// generated types serve as a reference for the wire shapes in `bodyFor()`.
// See: apps/palette-tauri/src/lib/axon-api.d.ts (generated)

import { invoke } from "./invoke";

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
  openResultsInline?: boolean;
  envValues?: Record<string, string | number | boolean | string[]>;
  configValues?: Record<string, string | number | boolean | string[]>;
}

export interface PaletteResult {
  ok: boolean;
  status: number;
  path: string;
  method: HttpMethod;
  payload: unknown;
}

export type HttpMethod = "GET" | "POST" | "DELETE";

export interface Client {
  baseUrl: string;
  headers: Record<string, string>;
}

export interface PaletteHttpRequest {
  baseUrl: string;
  token: string | null;
  method: HttpMethod;
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
    // Both runtimes route through the shared invoke wrapper: the Tauri bridge in
    // production, or a same-origin relative fetch (via the vite proxy) in browser
    // dev — never an absolute cross-origin URL.
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
): { method: HttpMethod; path: string; body: Record<string, unknown> | null } {
  const words = wordsFor(action, arg);
  const collection = config.collection.trim();
  const collectionBody = collection ? { collection } : {};
  const limit = config.resultLimit || 10;

  const lifecycle = jobLifecycleRequest(action.subcommand, words);
  if (lifecycle) return lifecycle;

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
      return { method: "POST", path: "/v1/summarize", body: { urls: required(words, "urls") } };
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
    case "chat":
      return { method: "POST", path: "/v1/chat", body: { message: first(words, "message") } };
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
    case "endpoints":
      return { method: "POST", path: "/v1/endpoints", body: { url: first(words, "url") } };
    case "brand":
      return { method: "POST", path: "/v1/brand", body: { url: first(words, "url") } };
    case "diff":
      return { method: "POST", path: "/v1/diff", body: diffBody(words) };
    case "screenshot":
      return {
        method: "POST",
        path: "/v1/screenshot",
        body: { url: first(words, "url"), full_page: true },
      };
    case "dedupe":
      return { method: "POST", path: "/v1/dedupe", body: collectionBody };
    case "watch-list":
      return { method: "GET", path: "/v1/watch", body: null };
    case "watch-create":
      return { method: "POST", path: "/v1/watch", body: watchCreateBody(words) };
    case "watch-run":
      return { method: "POST", path: `/v1/watch/${uuid(first(words, "watch id"))}/run`, body: null };
    case "ingest-sessions-prepared":
      return {
        method: "POST",
        path: "/v1/ingest/sessions/prepared",
        body: jsonBody(arg, "prepared sessions request"),
      };
    default:
      throw new Error(`REST route is not wired for ${action.subcommand}`);
  }
}

function jobLifecycleRequest(
  subcommand: string,
  words: string[],
): { method: HttpMethod; path: string; body: Record<string, unknown> | null } | null {
  const match = /^(crawl|embed|extract|ingest)-(list|status|cancel|cleanup|clear|recover)$/.exec(subcommand);
  if (!match) return null;
  const [, family, operation] = match;
  switch (operation) {
    case "list":
      return { method: "GET", path: `/v1/${family}`, body: null };
    case "status":
      return { method: "GET", path: `/v1/${family}/${uuid(first(words, "job id"))}`, body: null };
    case "cancel":
      return { method: "POST", path: `/v1/${family}/${uuid(first(words, "job id"))}/cancel`, body: null };
    case "cleanup":
      return { method: "POST", path: `/v1/${family}/cleanup`, body: null };
    case "clear":
      return { method: "DELETE", path: `/v1/${family}`, body: null };
    case "recover":
      return { method: "POST", path: `/v1/${family}/recover`, body: null };
    default:
      return null;
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
  return { source_type: "github", target, include_source: true };
}

function watchCreateBody(words: string[]): Record<string, unknown> {
  const url = first(words, "url");
  const seconds = words[1] ? Number(words[1]) : 3600;
  if (!Number.isFinite(seconds) || seconds < 1) {
    throw new Error("watch interval must be a positive number of seconds");
  }
  return {
    name: hostName(url),
    task_type: "watch",
    task_payload: { urls: [url], ignore_patterns: [] },
    every_seconds: Math.floor(seconds),
    enabled: true,
  };
}

function diffBody(words: string[]): Record<string, unknown> {
  const clean = required(words, "url_a and url_b");
  if (clean.length < 2) throw new Error("diff requires two URLs");
  return { url_a: clean[0], url_b: clean[1] };
}

function jsonBody(value: string, label: string): Record<string, unknown> {
  const parsed = JSON.parse(value.trim());
  if (!isRecord(parsed)) throw new Error(`${label} must be a JSON object`);
  return parsed;
}

function uuid(value: string): string {
  const clean = value.trim();
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(clean)) {
    throw new Error("id must be a UUID");
  }
  return clean;
}

function hostName(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function tokenFromHeaders(headers: Record<string, string>): string | null {
  const authorization = headers.Authorization;
  if (authorization?.startsWith("Bearer ")) {
    return authorization.slice("Bearer ".length);
  }
  return headers["x-api-key"] ?? null;
}

function failedResult(method: HttpMethod, path: string, error: unknown): PaletteResult {
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

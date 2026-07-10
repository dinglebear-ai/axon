// Per-action request shaping (HTTP method/path + body). These pure builders are
// bound per subcommand in `actionRegistry.ts` (`ActionBehavior.route` /
// `ActionBehavior.buildBody`); `axonClient.ts` composes them into the wire
// request. Splitting them here keeps the registry declarative and the client
// thin. No network calls or JSX here — pure shaping only.

import type { components } from "./axon-api";
import type { PaletteConfig } from "./axonClient";
import { splitShellWords } from "./shellWords";

/**
 * Generated OpenAPI request schemas — the single source of truth for `/v1/*`
 * request shapes. Each typed `BodyBuilder<Req[...]>` below is checked against
 * its route's schema at compile time, so a backend field rename/removal (which
 * `cargo xtask check-openapi-drift` regenerates into `axon-api.d.ts`) breaks the
 * palette build instead of failing silently at runtime.
 */
type Req = components["schemas"];

export type HttpMethod = "GET" | "POST" | "DELETE";

export interface ActionRouteTemplate {
  method: HttpMethod;
  path: string;
}

export type ArgMode = "none" | "optionalSingle" | "single" | "split";

/** Everything a body builder needs, derived once from the raw arg + config. */
export interface RequestContext {
  /** Raw argument string as typed by the user (used by JSON-body actions). */
  arg: string;
  /** Tokenized argument words, per the action's `argMode`. */
  words: string[];
  /** `{ collection }` when a collection is configured, else `{}`. Precisely
   * typed (not `Record`) so builders that spread it stay assignable to their
   * generated request schema. */
  collectionBody: { collection?: string };
  /** Effective result limit (>= 1). */
  limit: number;
}

export function buildRequestContext(argMode: ArgMode, arg: string, config: PaletteConfig): RequestContext {
  const collection = config.collection.trim();
  return {
    arg,
    words: wordsFor(argMode, arg),
    collectionBody: collection ? { collection } : {},
    limit: config.resultLimit || 10,
  };
}

function wordsFor(argMode: ArgMode, arg: string): string[] {
  switch (argMode) {
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

export type BodyBuilder<T = Record<string, unknown>> = (ctx: RequestContext) => T | null;

// ---- Static route helpers ------------------------------------------------

export const getRoute = (path: string): ActionRouteTemplate => ({ method: "GET", path });
export const postRoute = (path: string): ActionRouteTemplate => ({ method: "POST", path });
export const deleteRoute = (path: string): ActionRouteTemplate => ({ method: "DELETE", path });

export const noBody: BodyBuilder = () => null;

// ---- Per-action body builders -------------------------------------------

// scrape/crawl route through the unified `POST /v1/sources` pipeline (the
// verb-specific `/v1/scrape` and `/v1/crawl` routes were removed — see
// docs/pipeline-unification/surfaces/rest-contract.md and the CLI shim at
// crates/axon-cli/src/commands/source.rs, which this mirrors). `scope` hints
// single-page vs. full-site acquisition; the server still auto-classifies
// everything else. `SourceRequest.source` is singular, so — like the CLI —
// only the first URL is used even though the action accepts multiple words.
export const scrapeBody: BodyBuilder<Req["SourceRequest"]> = (ctx) => ({
  source: first(ctx.words, "url"),
  scope: "page",
  ...ctx.collectionBody,
});
export const crawlBody: BodyBuilder<Req["SourceRequest"]> = (ctx) => ({
  source: first(ctx.words, "url"),
  scope: "site",
  ...ctx.collectionBody,
});
export const mapBody: BodyBuilder<Req["RestMapRequest"]> = (ctx) => ({ url: first(ctx.words, "url") });
export const summarizeBody: BodyBuilder<Req["RestSummarizeRequest"]> = (ctx) => ({ urls: required(ctx.words, "urls") });
export const askBody: BodyBuilder<Req["RestAskRequest"]> = (ctx) => ({
  query: first(ctx.words, "query"),
  explain: false,
  diagnostics: false,
  ...ctx.collectionBody,
});
export const chatBody: BodyBuilder<Req["RestChatRequest"]> = (ctx) => ({ message: first(ctx.words, "message") });
export const queryBody: BodyBuilder<Req["RestQueryRequest"]> = (ctx) => ({ query: first(ctx.words, "query"), limit: ctx.limit, ...ctx.collectionBody });
export const retrieveBody: BodyBuilder<Req["RestRetrieveRequest"]> = (ctx) => ({ url: first(ctx.words, "url"), ...ctx.collectionBody });
export const suggestBody: BodyBuilder<Req["RestSuggestRequest"]> = (ctx) => (ctx.words[0] ? { focus: ctx.words[0] } : {});
export const evaluateBody: BodyBuilder<Req["RestEvaluateRequest"]> = (ctx) => ({ question: first(ctx.words, "question") });
export const searchBody: BodyBuilder<Req["RestSearchRequest"]> = (ctx) => ({ query: first(ctx.words, "query"), limit: ctx.limit });
export const researchBody: BodyBuilder<Req["RestResearchRequest"]> = (ctx) => ({ query: first(ctx.words, "query"), limit: ctx.limit });
// embed/ingest also route through `POST /v1/sources` (see the scrape/crawl
// comment above). Neither sets `scope` — the server auto-detects local path
// vs. URL vs. git/reddit/youtube/feed target via the canonical shared
// classifier (`classify_target`), the single source of truth across
// CLI/MCP/REST/palette. Do NOT reintroduce a client-side classifier here: it
// drifts from the backend (the old one only knew github/reddit/youtube and
// rejected gitlab/gitea/git/rss targets).
export const embedBody: BodyBuilder<Req["SourceRequest"]> = (ctx) => ({
  source: first(ctx.words, "input"),
  ...ctx.collectionBody,
});
export const extractBody: BodyBuilder<Req["RestExtractRequest"]> = (ctx) => ({ urls: required(ctx.words, "urls"), ...ctx.collectionBody });
export const ingestBody: BodyBuilder<Req["SourceRequest"]> = (ctx) => ({
  source: first(ctx.words, "target"),
  ...ctx.collectionBody,
});
export const endpointsBody: BodyBuilder<Req["EndpointsRequest"]> = (ctx) => ({ url: first(ctx.words, "url") });
export const brandBody: BodyBuilder<Req["RestBrandRequest"]> = (ctx) => ({ url: first(ctx.words, "url") });
export const diffBody: BodyBuilder<Req["RestDiffRequest"]> = (ctx) => diffRequestBody(ctx.words);
export const screenshotBody: BodyBuilder<Req["RestScreenshotRequest"]> = (ctx) => ({ url: first(ctx.words, "url"), full_page: true });
export const dedupeBody: BodyBuilder<Req["DedupeRequest"]> = (ctx) => ctx.collectionBody;
// Purge deletes by default (matches the CLI); the palette gates it behind a
// confirmation guard. `prefix`/`dry_run` default false server-side.
export const purgeBody: BodyBuilder<Req["PurgeRequest"]> = (ctx) => ({
  target: first(ctx.words, "target"),
  ...ctx.collectionBody,
});
export const watchCreateBody: BodyBuilder = (ctx) => watchCreateRequestBody(ctx.words);
export const ingestSessionsPreparedBody: BodyBuilder = (ctx) => jsonBody(ctx.arg, "prepared sessions request");
// `github` takes a bare owner[/repo[/path...]] target (NOT a URL — see
// `BARE_TARGET_SUBCOMMANDS`-style handling in actions.ts, though github is
// simply absent from `acceptsDirectUrl` so no coercion ever applies). This
// body is consumed directly by `executeAction`'s github special-case (see
// axonClient.ts), not by `axon_http_request` — the route is a `palette://`
// marker, never a real Axon REST call.
export const githubBrowseBody: BodyBuilder<GitHubBrowseRequestBody> = (ctx) =>
  parseGitHubTarget(first(ctx.words, "owner or owner/repo[/path]"));

export type GitHubBrowseRequestBody = {
  kind: "repos" | "tree" | "file" | "feed";
  owner: string;
  repo?: string;
  path?: string;
} & Record<string, unknown>;

/** Wire shape returned by the `github_browse` Tauri command
 * (`src-tauri/src/github_bridge.rs::GitHubBrowseResult`). Field names are
 * `camelCase` because the Rust struct is `#[serde(rename_all = "camelCase")]`.
 * Lives in the lib layer (not the component) so both `axonClient.ts` and
 * `GitHubView.tsx` share one definition without a component → lib inversion. */
export interface GitHubBrowseResult {
  ok: boolean;
  status: number;
  kind: string;
  owner: string;
  repo: string | null;
  branch: string | null;
  path: string | null;
  payload: unknown;
  error: string | null;
  rateLimitRemaining: number | null;
  rateLimitReset: number | null;
  authenticated: boolean;
}

/**
 * Parse a `github` action argument into a browse request.
 *
 * - `owner` → list the owner's repos (`kind: "repos"`).
 * - `owner/repo` → the repo's full file tree (`kind: "tree"`).
 * - `owner/repo/some/path.ext` → preview that file's contents (`kind: "file"`).
 */
export function parseGitHubTarget(target: string): GitHubBrowseRequestBody {
  const trimmed = target.trim().replace(/^\/+|\/+$/g, "");
  if (!trimmed) throw new Error("owner or owner/repo[/path] is required");
  const segments = trimmed.split("/").filter(Boolean);
  const [owner, repo, ...rest] = segments;
  if (!owner) throw new Error("owner is required");
  if (!repo) return { kind: "repos", owner };
  if (rest.length === 0) return { kind: "tree", owner, repo };
  return { kind: "file", owner, repo, path: rest.join("/") };
}

// ---- Shared shaping helpers ---------------------------------------------

export function first(words: string[], field: string): string {
  return required(words, field)[0];
}

export function required(words: string[], field: string): string[] {
  const clean = words.map((word) => word.trim()).filter(Boolean);
  if (!clean.length) throw new Error(`${field} is required`);
  return clean;
}

function watchCreateRequestBody(words: string[]): Record<string, unknown> {
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

function diffRequestBody(words: string[]): Req["RestDiffRequest"] {
  const clean = required(words, "url_a and url_b");
  if (clean.length < 2) throw new Error("diff requires two URLs");
  return { url_a: clean[0], url_b: clean[1] };
}

function jsonBody(value: string, label: string): Record<string, unknown> {
  const parsed = JSON.parse(value.trim());
  if (!isRecord(parsed)) throw new Error(`${label} must be a JSON object`);
  return parsed;
}

/** Validate a string is a UUID, throwing a user-facing error otherwise. */
export function uuid(value: string): string {
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

// Per-action request shaping (HTTP method/path + body). These pure builders are
// bound per subcommand in `actionRegistry.ts` (`ActionBehavior.route` /
// `ActionBehavior.buildBody`); `axonClient.ts` composes them into the wire
// request. Splitting them here keeps the registry declarative and the client
// thin. No network calls or JSX here — pure shaping only.

import type { PaletteConfig } from "./axonClient";
import { splitShellWords } from "./shellWords";

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
  /** `{ collection }` when a collection is configured, else `{}`. */
  collectionBody: Record<string, unknown>;
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

export type BodyBuilder = (ctx: RequestContext) => Record<string, unknown> | null;

// ---- Static route helpers ------------------------------------------------

export const getRoute = (path: string): ActionRouteTemplate => ({ method: "GET", path });
export const postRoute = (path: string): ActionRouteTemplate => ({ method: "POST", path });
export const deleteRoute = (path: string): ActionRouteTemplate => ({ method: "DELETE", path });

export const noBody: BodyBuilder = () => null;

// ---- Per-action body builders -------------------------------------------

export const scrapeBody: BodyBuilder = (ctx) => ({ url: first(ctx.words, "url"), ...ctx.collectionBody });
export const crawlBody: BodyBuilder = (ctx) => ({ urls: required(ctx.words, "urls"), ...ctx.collectionBody });
export const mapBody: BodyBuilder = (ctx) => ({ url: first(ctx.words, "url") });
export const summarizeBody: BodyBuilder = (ctx) => ({ urls: required(ctx.words, "urls") });
export const askBody: BodyBuilder = (ctx) => ({
  query: first(ctx.words, "query"),
  explain: false,
  diagnostics: false,
  ...ctx.collectionBody,
});
export const chatBody: BodyBuilder = (ctx) => ({ message: first(ctx.words, "message") });
export const queryBody: BodyBuilder = (ctx) => ({ query: first(ctx.words, "query"), limit: ctx.limit, ...ctx.collectionBody });
export const retrieveBody: BodyBuilder = (ctx) => ({ url: first(ctx.words, "url"), ...ctx.collectionBody });
export const suggestBody: BodyBuilder = (ctx) => (ctx.words[0] ? { focus: ctx.words[0] } : {});
export const evaluateBody: BodyBuilder = (ctx) => ({ question: first(ctx.words, "question") });
export const searchBody: BodyBuilder = (ctx) => ({ query: first(ctx.words, "query"), limit: ctx.limit });
export const researchBody: BodyBuilder = (ctx) => ({ query: first(ctx.words, "query"), limit: ctx.limit });
export const embedBody: BodyBuilder = (ctx) => ({ input: first(ctx.words, "input"), ...ctx.collectionBody });
export const extractBody: BodyBuilder = (ctx) => ({ urls: required(ctx.words, "urls"), ...ctx.collectionBody });
export const ingestBody: BodyBuilder = (ctx) => ingestTargetBody(first(ctx.words, "target"));
export const endpointsBody: BodyBuilder = (ctx) => ({ url: first(ctx.words, "url") });
export const brandBody: BodyBuilder = (ctx) => ({ url: first(ctx.words, "url") });
export const diffBody: BodyBuilder = (ctx) => diffRequestBody(ctx.words);
export const screenshotBody: BodyBuilder = (ctx) => ({ url: first(ctx.words, "url"), full_page: true });
export const dedupeBody: BodyBuilder = (ctx) => ctx.collectionBody;
export const watchCreateBody: BodyBuilder = (ctx) => watchCreateRequestBody(ctx.words);
export const ingestSessionsPreparedBody: BodyBuilder = (ctx) => jsonBody(ctx.arg, "prepared sessions request");

// ---- Shared shaping helpers ---------------------------------------------

export function first(words: string[], field: string): string {
  return required(words, field)[0];
}

export function required(words: string[], field: string): string[] {
  const clean = words.map((word) => word.trim()).filter(Boolean);
  if (!clean.length) throw new Error(`${field} is required`);
  return clean;
}

function ingestTargetBody(target: string): Record<string, unknown> {
  const lower = target.toLowerCase();
  const host = targetHost(lower);
  if (hostMatches(host, "youtube.com") || host === "youtu.be") {
    return { source_type: "youtube", target };
  }
  if (hostMatches(host, "reddit.com") || lower.startsWith("/r/") || lower.startsWith("r/")) {
    return { source_type: "reddit", target };
  }
  return { source_type: "github", target, include_source: true };
}

function targetHost(target: string): string | null {
  try {
    return new URL(target).hostname.toLowerCase();
  } catch {
    return null;
  }
}

function hostMatches(host: string | null, domain: string): boolean {
  return host === domain || Boolean(host?.endsWith(`.${domain}`));
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

function diffRequestBody(words: string[]): Record<string, unknown> {
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

// NOTE (M1): OpenAPI types are generated into `./axon-api.d.ts` via
// `pnpm generate:api`.  Request execution is currently hand-coded here; a full
// migration to the generated request helpers (`openapi-fetch`) is tracked in
// GitHub issue #177 (finding M1).  Until that migration is complete the
// generated types serve as a reference for the wire shapes in `bodyFor()`.
// See: apps/palette-tauri/src/lib/axon-api.d.ts (generated)

import { invoke } from "./invoke";

import type { PaletteAction, PaletteSubcommand, RemotePaletteAction } from "./actions";
import { ACTION_REGISTRY } from "./actionRegistry";
import { buildRequestContext, type ActionRouteTemplate, type HttpMethod } from "./actionRequest";

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

/**
 * Request body the REST layer builds for an action. The `payload` of a
 * successful response is still key-probed by the result views (the generated
 * OpenAPI response types are not yet wired — tracked in #177); these aliases
 * exist so request construction is typed without changing any response shape a
 * consumer reads.
 */
export interface RequestBody {
  method: HttpMethod;
  path: string;
  body: Record<string, unknown> | null;
}

/** A successful (`ok: true`) REST result. Additive alias over `PaletteResult`. */
export type SuccessResult = PaletteResult & { ok: true };

/** A failed (`ok: false`) REST result. Additive alias over `PaletteResult`. */
export type ErrorResult = PaletteResult & { ok: false };

// HttpMethod / ActionRouteTemplate now live in actionRequest.ts (the request
// shaping module). Re-exported here for existing importers (actionMeta, etc.).
export type { HttpMethod, ActionRouteTemplate };

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
  action: RemotePaletteAction,
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
  action: RemotePaletteAction,
  arg: string,
  config: PaletteConfig,
): PaletteHttpRequest {
  if ((action as PaletteAction).kind === "local") {
    throw new Error(`Local action ${action.subcommand} cannot be sent to Axon REST`);
  }
  const body = bodyFor(action, arg, config);
  return {
    baseUrl: client.baseUrl,
    token: tokenFromHeaders(client.headers),
    method: body.method,
    path: body.path,
    body: body.body,
  };
}

// Request shaping is now driven by the per-action registry
// (`ACTION_REGISTRY[subcommand]`): the route template, an optional id-aware
// route resolver (`routeFor`), and the body builder all live there. This is the
// single source of truth for A-H1 — a new subcommand must declare a complete
// behavior entry or the registry's `Record` fails to type-check.
function bodyFor(action: RemotePaletteAction, arg: string, config: PaletteConfig): RequestBody {
  const behavior = ACTION_REGISTRY[action.subcommand];
  const ctx = buildRequestContext(action.argMode, arg, config);
  const route = behavior.routeFor ? behavior.routeFor(ctx) : behavior.route;
  return { method: route.method, path: route.path, body: behavior.buildBody(ctx) };
}

/**
 * Route template (method + templated path, e.g. `/v1/crawl/{id}`) for a
 * subcommand — used by `actionMeta` for display. Delegates to the registry.
 */
export function actionRouteTemplate(subcommand: string): ActionRouteTemplate {
  return ACTION_REGISTRY[subcommand as PaletteSubcommand]?.route ?? { method: "POST", path: `/v1/${subcommand}` };
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

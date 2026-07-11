// Single source of truth for per-action behavior. `ACTION_REGISTRY` is keyed by
// the full subcommand union so every new action must define route/body/output/
// icon/structured-view behavior before TypeScript accepts it.

import {
  Activity,
  BarChart3,
  BookOpen,
  Bot,
  Boxes,
  Braces,
  Camera,
  Database,
  FileDown,
  FolderGit2,
  FolderOpen,
  GitBranch,
  GitCompare,
  Globe,
  Compass,
  HelpCircle,
  Layers,
  Map as MapIcon,
  PackageOpen,
  SearchCheck,
  Sparkles,
  Stethoscope,
  TerminalSquare,
  Trash2,
  Workflow,
  type LucideIcon,
} from "lucide-react";

import type { PaletteSubcommand, JobFamily, JobOperation } from "./actions";
import {
  type ActionRouteTemplate,
  type BodyBuilder,
  type RequestContext,
  askBody,
  brandBody,
  chatBody,
  crawlBody,
  dedupeBody,
  purgeBody,
  diffBody,
  embedBody,
  endpointsBody,
  evaluateBody,
  extractBody,
  first,
  getRoute,
  githubBrowseBody,
  ingestBody,
  ingestSessionsPreparedBody,
  mapBody,
  noBody,
  postRoute,
  queryBody,
  researchBody,
  retrieveBody,
  scrapeBody,
  screenshotBody,
  searchBody,
  suggestBody,
  summarizeBody,
  uuid,
  watchCreateBody,
} from "./actionRequest";
import {
  type RecordFormatter,
  formatAnswer,
  formatBrand,
  formatCompact,
  formatDedupe,
  formatPurge,
  formatDiff,
  formatDomains,
  formatEndpoints,
  formatEvaluate,
  formatGitHub,
  formatMap,
  formatQuery,
  formatRetrieve,
  formatScrape,
  formatScreenshot,
  formatSearchLike,
  formatSources,
  formatSuggest,
  formatSummarize,
  formatWatchCreate,
  formatWatchList,
  formatWatchRun,
  jobStartFormatter,
  recordFormatter,
} from "./actionFormat";
import { buildLifecycleRegistry } from "./actionLifecycle";

export type OutputKind = "markdown" | "code";

/**
 * Keys for the structured result views rendered in `OperationResultView.tsx`.
 * `null` means "no structured view" — either a plain markdown/code body
 * (`outputKind`) or a top-level view handled directly by `OutputPanel`
 * (evaluate/stats/status). The `STRUCTURED_VIEWS` map must define exactly these
 * keys (enforced by an exhaustiveness test).
 */
export type StructuredViewKey =
  | "help"
  | "files"
  | "scrape"
  | "query"
  | "retrieve"
  | "search"
  | "research"
  | "map"
  | "suggest"
  | "sources"
  | "domains"
  | "doctor"
  | "crawl"
  | "embed"
  | "extract"
  | "ingest"
  | "ingest-sessions-prepared"
  | "github"
  | "endpoints"
  | "brand"
  | "diff"
  | "screenshot"
  | "dedupe"
  | "watch-list"
  | "watch-create"
  | "watch-run"
  | "job-lifecycle";

export interface ActionBehavior {
  /** HTTP route template (path may contain `{id}` placeholders). */
  route: ActionRouteTemplate;
  /** Builds the request body from the tokenized arg + config, or `null`. */
  buildBody: BodyBuilder;
  /** If set, the route is computed from the request context (per-id paths). */
  routeFor?: (ctx: RequestContext) => ActionRouteTemplate;
  /** Whether the result body renders as markdown or a code block. */
  outputKind: OutputKind;
  /** Fallback / copy text formatter for the result payload. */
  formatText: (payload: unknown) => string;
  /** Icon shown in the action list / command bar. */
  actionIcon: LucideIcon;
  /** Icon shown in the output panel header. */
  outputIcon: LucideIcon;
  /**
   * Structured-view key resolved by `OperationResultView.tsx`, or `null` when
   * the action has no structured view (markdown/code body, or a top-level view
   * owned by `OutputPanel`).
   */
  structuredView: StructuredViewKey | null;
}

const md: OutputKind = "markdown";
const code: OutputKind = "code";

type ActionBehaviorInput = Omit<ActionBehavior, "outputIcon"> & { outputIcon?: LucideIcon };

/** Helper to declare an entry; defaults `outputIcon` to `actionIcon`. */
function behavior(input: ActionBehaviorInput): ActionBehavior {
  return { ...input, outputIcon: input.outputIcon ?? input.actionIcon };
}

/** Helper to declare a record-formatting entry; binds the formatter guard. */
function entry(input: Omit<ActionBehaviorInput, "formatText"> & { formatText: RecordFormatter }): ActionBehavior {
  return behavior({ ...input, formatText: recordFormatter(input.formatText) });
}

// Non-lifecycle subcommands. Job-lifecycle entries are generated below so the
// `Record` stays exhaustive without 24 hand-written rows.
type StaticSubcommand = Exclude<PaletteSubcommand, `${JobFamily}-${JobOperation}`>;

const STATIC_REGISTRY: Record<StaticSubcommand, ActionBehavior> = {
  help: behavior({
    route: getRoute("palette://help"),
    buildBody: noBody,
    outputKind: code,
    formatText: formatCompact,
    actionIcon: HelpCircle,
    structuredView: "help",
  }),
  // Browser is a local, window-driven action (Tauri webview creation +
  // navigation commands), so it uses a palette marker route for metadata only.
  browser: behavior({
    route: getRoute("palette://browser"),
    buildBody: noBody,
    outputKind: code,
    formatText: formatCompact,
    actionIcon: Compass,
    structuredView: null,
  }),
  files: behavior({
    route: getRoute("palette://files"),
    buildBody: noBody,
    outputKind: code,
    formatText: formatCompact,
    actionIcon: FolderOpen,
    structuredView: "files",
  }),
  scrape: entry({
    // Verb route removed server-side; routes through the unified source
    // pipeline (see the comment on `scrapeBody` in actionRequest.ts).
    route: postRoute("/v1/sources"),
    buildBody: scrapeBody,
    outputKind: md,
    formatText: formatScrape,
    actionIcon: FileDown,
    structuredView: "scrape",
  }),
  crawl: entry({
    // Verb route removed server-side; routes through the unified source
    // pipeline (see the comment on `crawlBody` in actionRequest.ts).
    route: postRoute("/v1/sources"),
    buildBody: crawlBody,
    outputKind: code,
    formatText: jobStartFormatter("crawl"),
    actionIcon: Workflow,
    outputIcon: GitBranch,
    structuredView: "crawl",
  }),
  map: entry({
    route: postRoute("/v1/map"),
    buildBody: mapBody,
    outputKind: code,
    formatText: formatMap,
    actionIcon: MapIcon,
    structuredView: "map",
  }),
  summarize: entry({
    route: postRoute("/v1/summarize"),
    buildBody: summarizeBody,
    outputKind: md,
    formatText: formatSummarize,
    actionIcon: BookOpen,
    structuredView: null,
  }),
  ask: entry({
    route: postRoute("/v1/ask"),
    buildBody: askBody,
    outputKind: md,
    formatText: formatAnswer,
    actionIcon: Bot,
    structuredView: null,
  }),
  chat: entry({
    route: postRoute("/v1/chat"),
    buildBody: chatBody,
    outputKind: md,
    formatText: formatAnswer,
    actionIcon: Bot,
    outputIcon: HelpCircle,
    structuredView: null,
  }),
  query: entry({
    route: postRoute("/v1/query"),
    buildBody: queryBody,
    outputKind: code,
    formatText: formatQuery,
    actionIcon: SearchCheck,
    structuredView: "query",
  }),
  retrieve: entry({
    route: postRoute("/v1/retrieve"),
    buildBody: retrieveBody,
    outputKind: code,
    formatText: formatRetrieve,
    actionIcon: Database,
    structuredView: "retrieve",
  }),
  suggest: entry({
    route: postRoute("/v1/suggest"),
    buildBody: suggestBody,
    outputKind: md,
    formatText: formatSuggest,
    actionIcon: Sparkles,
    structuredView: "suggest",
  }),
  evaluate: entry({
    route: postRoute("/v1/evaluate"),
    buildBody: evaluateBody,
    outputKind: code,
    formatText: formatEvaluate,
    actionIcon: BarChart3,
    structuredView: null,
  }),
  search: entry({
    route: postRoute("/v1/search"),
    buildBody: searchBody,
    outputKind: code,
    formatText: formatSearchLike,
    actionIcon: Globe,
    structuredView: "search",
  }),
  research: entry({
    route: postRoute("/v1/research"),
    buildBody: researchBody,
    outputKind: md,
    formatText: formatSearchLike,
    actionIcon: Globe,
    structuredView: "research",
  }),
  embed: entry({
    // Verb route removed server-side; routes through the unified source
    // pipeline (see the comment on `embedBody` in actionRequest.ts).
    route: postRoute("/v1/sources"),
    buildBody: embedBody,
    outputKind: code,
    formatText: jobStartFormatter("embed"),
    actionIcon: Layers,
    structuredView: "embed",
  }),
  extract: entry({
    route: postRoute("/v1/extract"),
    buildBody: extractBody,
    outputKind: code,
    formatText: jobStartFormatter("extract"),
    actionIcon: Braces,
    structuredView: "extract",
  }),
  ingest: entry({
    // Verb route removed server-side; routes through the unified source
    // pipeline (see the comment on `ingestBody` in actionRequest.ts).
    route: postRoute("/v1/sources"),
    buildBody: ingestBody,
    outputKind: code,
    formatText: jobStartFormatter("ingest"),
    actionIcon: PackageOpen,
    structuredView: "ingest",
  }),
  // `github` is NOT an Axon REST call — the route is a `palette://` marker so
  // it's inert if it were ever accidentally sent to `axon_http_request`
  // (rejected: doesn't start with `/v1/`). `executeAction` special-cases this
  // subcommand to call the `github_browse` Tauri command instead (see
  // axonClient.ts), which proxies `api.github.com` from the Rust side per the
  // desktop CSP (`connect-src` has no api.github.com origin for the renderer).
  github: entry({
    route: getRoute("palette://github"),
    buildBody: githubBrowseBody,
    outputKind: code,
    formatText: formatGitHub,
    actionIcon: FolderGit2,
    structuredView: "github",
  }),
  status: behavior({
    route: getRoute("/v1/status"),
    buildBody: noBody,
    outputKind: code,
    formatText: formatCompact,
    actionIcon: Activity,
    structuredView: null,
  }),
  sources: entry({
    route: getRoute("/v1/sources"),
    buildBody: noBody,
    outputKind: code,
    formatText: formatSources,
    actionIcon: Boxes,
    structuredView: "sources",
  }),
  domains: entry({
    route: getRoute("/v1/domains"),
    buildBody: noBody,
    outputKind: code,
    formatText: formatDomains,
    actionIcon: Database,
    structuredView: "domains",
  }),
  stats: behavior({
    route: getRoute("/v1/stats"),
    buildBody: noBody,
    outputKind: code,
    formatText: formatCompact,
    actionIcon: BarChart3,
    structuredView: null,
  }),
  doctor: entry({
    route: getRoute("/v1/doctor"),
    buildBody: noBody,
    outputKind: code,
    formatText: formatCompact,
    actionIcon: Stethoscope,
    structuredView: "doctor",
  }),
  endpoints: entry({
    route: postRoute("/v1/endpoints"),
    buildBody: endpointsBody,
    outputKind: md,
    formatText: formatEndpoints,
    actionIcon: HelpCircle,
    structuredView: "endpoints",
  }),
  brand: entry({
    route: postRoute("/v1/brand"),
    buildBody: brandBody,
    outputKind: md,
    formatText: formatBrand,
    actionIcon: Sparkles,
    structuredView: "brand",
  }),
  diff: entry({
    route: postRoute("/v1/diff"),
    buildBody: diffBody,
    outputKind: md,
    formatText: formatDiff,
    actionIcon: GitCompare,
    structuredView: "diff",
  }),
  screenshot: entry({
    route: postRoute("/v1/screenshot"),
    buildBody: screenshotBody,
    outputKind: md,
    formatText: formatScreenshot,
    actionIcon: Camera,
    structuredView: "screenshot",
  }),
  dedupe: entry({
    route: postRoute("/v1/dedupe"),
    buildBody: dedupeBody,
    outputKind: code,
    formatText: formatDedupe,
    actionIcon: HelpCircle,
    structuredView: "dedupe",
  }),
  purge: entry({
    route: postRoute("/v1/purge"),
    buildBody: purgeBody,
    outputKind: code,
    formatText: formatPurge,
    actionIcon: Trash2,
    structuredView: null,
  }),
  "watch-list": entry({
    route: getRoute("/v1/watch"),
    buildBody: noBody,
    outputKind: code,
    formatText: formatWatchList,
    actionIcon: HelpCircle,
    structuredView: "watch-list",
  }),
  "watch-create": entry({
    route: postRoute("/v1/watch"),
    buildBody: watchCreateBody,
    outputKind: code,
    formatText: formatWatchCreate,
    actionIcon: HelpCircle,
    structuredView: "watch-create",
  }),
  "watch-run": entry({
    route: postRoute("/v1/watch/{id}/run"),
    buildBody: noBody,
    routeFor: (ctx) => postRoute(`/v1/watch/${uuid(first(ctx.words, "watch id"))}/run`),
    outputKind: code,
    formatText: formatWatchRun,
    actionIcon: HelpCircle,
    structuredView: "watch-run",
  }),
  "ingest-sessions-prepared": entry({
    route: postRoute("/v1/ingest/sessions/prepared"),
    buildBody: ingestSessionsPreparedBody,
    outputKind: code,
    formatText: jobStartFormatter("ingest-sessions-prepared"),
    actionIcon: HelpCircle,
    structuredView: "ingest-sessions-prepared",
  }),
  // Local Tauri shell action; the marker route is metadata-only.
  terminal: behavior({
    route: getRoute("palette://terminal"),
    buildBody: noBody,
    outputKind: code,
    formatText: formatCompact,
    actionIcon: TerminalSquare,
    structuredView: null,
  }),
};

/** Exhaustive per-action behavior table. Keyed by the full subcommand union. */
export const ACTION_REGISTRY: Record<PaletteSubcommand, ActionBehavior> = {
  ...STATIC_REGISTRY,
  ...buildLifecycleRegistry(),
};

/** Nullable lookup for persisted/loaded data where the subcommand may be stale. */
export function maybeActionBehavior(subcommand: string): ActionBehavior | null {
  return ACTION_REGISTRY[subcommand as PaletteSubcommand] ?? null;
}

/** Behavior for a subcommand. String call sites are runtime boundaries (history,
 * URL params, user-visible helpers), so fail loudly instead of silently drifting
 * to generic JSON or plausible-looking REST routes. */
export function actionBehavior(subcommand: PaletteSubcommand): ActionBehavior;
export function actionBehavior(subcommand: string): ActionBehavior;
export function actionBehavior(subcommand: string): ActionBehavior {
  const action = maybeActionBehavior(subcommand);
  if (!action) throw new Error(`Unknown palette action: ${subcommand}`);
  return action;
}

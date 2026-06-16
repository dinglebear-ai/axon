// Single source of truth for per-action behavior (finding A-H1).
//
// Before this registry, adding a palette action meant editing ~9 parallel
// dispatch sites (request body, route, output kind, fallback text formatter, two
// icon maps, the structured-view allowlist) with no compile-time guarantee they
// stayed in sync — a forgotten site silently degraded to raw `<pre>` JSON.
//
// `ACTION_REGISTRY` is a `Record<PaletteSubcommand, ActionBehavior>`: because it
// is keyed by the full union (including the 24 `${JobFamily}-${JobOperation}`
// members, generated below), a new subcommand fails to type-check until it has a
// complete behavior entry. The scattered functions (`bodyFor`, `actionRouteTemplate`,
// `outputKindFor`, `formatPayload`, `outputIcon`, `actionIcon`,
// `hasStructuredOperationView`) all derive from this map.
//
// Structured-view rendering (JSX) cannot live in this `.ts` file, so each entry
// carries a `structuredView` *key* (or `null`); the `STRUCTURED_VIEWS` renderer
// map in `OperationResultView.tsx` is keyed by the same `StructuredViewKey`
// union, and a test asserts the two stay in lockstep.

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
  GitBranch,
  GitCompare,
  Globe,
  HelpCircle,
  Layers,
  Map as MapIcon,
  PackageOpen,
  RotateCw,
  SearchCheck,
  Sparkles,
  Stethoscope,
  Workflow,
  X,
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
  deleteRoute,
  diffBody,
  embedBody,
  endpointsBody,
  evaluateBody,
  extractBody,
  first,
  getRoute,
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
  formatDiff,
  formatDomains,
  formatEndpoints,
  formatEvaluate,
  formatJobLifecycle,
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

/** Helper to declare an entry; binds a record-only formatter through the guard. */
function entry(behavior: Omit<ActionBehavior, "formatText"> & { formatText: RecordFormatter }): ActionBehavior {
  return { ...behavior, formatText: recordFormatter(behavior.formatText) };
}

// Non-lifecycle subcommands. Job-lifecycle entries are generated below so the
// `Record` stays exhaustive without 24 hand-written rows.
type StaticSubcommand = Exclude<PaletteSubcommand, `${JobFamily}-${JobOperation}`>;

const STATIC_REGISTRY: Record<StaticSubcommand, ActionBehavior> = {
  help: {
    route: getRoute("palette://help"),
    buildBody: noBody,
    outputKind: code,
    formatText: formatCompact,
    actionIcon: HelpCircle,
    outputIcon: HelpCircle,
    structuredView: "help",
  },
  scrape: entry({
    route: postRoute("/v1/scrape"),
    buildBody: scrapeBody,
    outputKind: md,
    formatText: formatScrape,
    actionIcon: FileDown,
    outputIcon: FileDown,
    structuredView: "scrape",
  }),
  crawl: entry({
    route: postRoute("/v1/crawl"),
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
    outputIcon: MapIcon,
    structuredView: "map",
  }),
  summarize: entry({
    route: postRoute("/v1/summarize"),
    buildBody: summarizeBody,
    outputKind: md,
    formatText: formatSummarize,
    actionIcon: BookOpen,
    outputIcon: BookOpen,
    structuredView: null,
  }),
  ask: entry({
    route: postRoute("/v1/ask"),
    buildBody: askBody,
    outputKind: md,
    formatText: formatAnswer,
    actionIcon: Bot,
    outputIcon: Bot,
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
    outputIcon: SearchCheck,
    structuredView: "query",
  }),
  retrieve: entry({
    route: postRoute("/v1/retrieve"),
    buildBody: retrieveBody,
    outputKind: code,
    formatText: formatRetrieve,
    actionIcon: Database,
    outputIcon: Database,
    structuredView: "retrieve",
  }),
  suggest: entry({
    route: postRoute("/v1/suggest"),
    buildBody: suggestBody,
    outputKind: md,
    formatText: formatSuggest,
    actionIcon: Sparkles,
    outputIcon: Sparkles,
    structuredView: "suggest",
  }),
  evaluate: entry({
    route: postRoute("/v1/evaluate"),
    buildBody: evaluateBody,
    outputKind: code,
    formatText: formatEvaluate,
    actionIcon: BarChart3,
    outputIcon: BarChart3,
    structuredView: null,
  }),
  search: entry({
    route: postRoute("/v1/search"),
    buildBody: searchBody,
    outputKind: code,
    formatText: formatSearchLike,
    actionIcon: Globe,
    outputIcon: Globe,
    structuredView: "search",
  }),
  research: entry({
    route: postRoute("/v1/research"),
    buildBody: researchBody,
    outputKind: md,
    formatText: formatSearchLike,
    actionIcon: Globe,
    outputIcon: Globe,
    structuredView: "research",
  }),
  embed: entry({
    route: postRoute("/v1/embed"),
    buildBody: embedBody,
    outputKind: code,
    formatText: jobStartFormatter("embed"),
    actionIcon: Layers,
    outputIcon: Layers,
    structuredView: "embed",
  }),
  extract: entry({
    route: postRoute("/v1/extract"),
    buildBody: extractBody,
    outputKind: code,
    formatText: jobStartFormatter("extract"),
    actionIcon: Braces,
    outputIcon: Braces,
    structuredView: "extract",
  }),
  ingest: entry({
    route: postRoute("/v1/ingest"),
    buildBody: ingestBody,
    outputKind: code,
    formatText: jobStartFormatter("ingest"),
    actionIcon: PackageOpen,
    outputIcon: PackageOpen,
    structuredView: "ingest",
  }),
  status: {
    route: getRoute("/v1/status"),
    buildBody: noBody,
    outputKind: code,
    formatText: formatCompact,
    actionIcon: Activity,
    outputIcon: Activity,
    structuredView: null,
  },
  sources: entry({
    route: getRoute("/v1/sources"),
    buildBody: noBody,
    outputKind: code,
    formatText: formatSources,
    actionIcon: Boxes,
    outputIcon: Boxes,
    structuredView: "sources",
  }),
  domains: entry({
    route: getRoute("/v1/domains"),
    buildBody: noBody,
    outputKind: code,
    formatText: formatDomains,
    actionIcon: Database,
    outputIcon: Database,
    structuredView: "domains",
  }),
  stats: {
    route: getRoute("/v1/stats"),
    buildBody: noBody,
    outputKind: code,
    formatText: formatCompact,
    actionIcon: BarChart3,
    outputIcon: BarChart3,
    structuredView: null,
  },
  doctor: entry({
    route: getRoute("/v1/doctor"),
    buildBody: noBody,
    outputKind: code,
    formatText: formatCompact,
    actionIcon: Stethoscope,
    outputIcon: Stethoscope,
    structuredView: "doctor",
  }),
  endpoints: entry({
    route: postRoute("/v1/endpoints"),
    buildBody: endpointsBody,
    outputKind: md,
    formatText: formatEndpoints,
    actionIcon: HelpCircle,
    outputIcon: HelpCircle,
    structuredView: "endpoints",
  }),
  brand: entry({
    route: postRoute("/v1/brand"),
    buildBody: brandBody,
    outputKind: md,
    formatText: formatBrand,
    actionIcon: Sparkles,
    outputIcon: Sparkles,
    structuredView: "brand",
  }),
  diff: entry({
    route: postRoute("/v1/diff"),
    buildBody: diffBody,
    outputKind: md,
    formatText: formatDiff,
    actionIcon: GitCompare,
    outputIcon: GitCompare,
    structuredView: "diff",
  }),
  screenshot: entry({
    route: postRoute("/v1/screenshot"),
    buildBody: screenshotBody,
    outputKind: md,
    formatText: formatScreenshot,
    actionIcon: Camera,
    outputIcon: Camera,
    structuredView: "screenshot",
  }),
  dedupe: entry({
    route: postRoute("/v1/dedupe"),
    buildBody: dedupeBody,
    outputKind: code,
    formatText: formatDedupe,
    actionIcon: HelpCircle,
    outputIcon: HelpCircle,
    structuredView: "dedupe",
  }),
  "watch-list": entry({
    route: getRoute("/v1/watch"),
    buildBody: noBody,
    outputKind: code,
    formatText: formatWatchList,
    actionIcon: HelpCircle,
    outputIcon: HelpCircle,
    structuredView: "watch-list",
  }),
  "watch-create": entry({
    route: postRoute("/v1/watch"),
    buildBody: watchCreateBody,
    outputKind: code,
    formatText: formatWatchCreate,
    actionIcon: HelpCircle,
    outputIcon: HelpCircle,
    structuredView: "watch-create",
  }),
  "watch-run": entry({
    route: postRoute("/v1/watch/{id}/run"),
    buildBody: noBody,
    routeFor: (ctx) => postRoute(`/v1/watch/${uuid(first(ctx.words, "watch id"))}/run`),
    outputKind: code,
    formatText: formatWatchRun,
    actionIcon: HelpCircle,
    outputIcon: HelpCircle,
    structuredView: "watch-run",
  }),
  "ingest-sessions-prepared": entry({
    route: postRoute("/v1/ingest/sessions/prepared"),
    buildBody: ingestSessionsPreparedBody,
    outputKind: code,
    formatText: jobStartFormatter("ingest-sessions-prepared"),
    actionIcon: HelpCircle,
    outputIcon: HelpCircle,
    structuredView: "ingest-sessions-prepared",
  }),
};

const JOB_LIFECYCLE_ICONS: Record<JobFamily, LucideIcon> = {
  crawl: GitBranch,
  embed: Layers,
  extract: Braces,
  ingest: PackageOpen,
};

function lifecycleBehavior(family: JobFamily, operation: JobOperation): ActionBehavior {
  const route: ActionRouteTemplate =
    operation === "list"
      ? getRoute(`/v1/${family}`)
      : operation === "status"
        ? getRoute(`/v1/${family}/{id}`)
        : operation === "cancel"
          ? postRoute(`/v1/${family}/{id}/cancel`)
          : operation === "cleanup"
            ? postRoute(`/v1/${family}/cleanup`)
            : operation === "clear"
              ? deleteRoute(`/v1/${family}`)
              : postRoute(`/v1/${family}/recover`);

  // Body is always null for lifecycle ops; `routeFor` performs UUID validation
  // (status/cancel) — matching the original `jobLifecycleRequest` behavior.
  const routeFor =
    operation === "status"
      ? (ctx: RequestContext) => getRoute(`/v1/${family}/${uuid(first(ctx.words, "job id"))}`)
      : operation === "cancel"
        ? (ctx: RequestContext) => postRoute(`/v1/${family}/${uuid(first(ctx.words, "job id"))}/cancel`)
        : undefined;

  const icon = JOB_LIFECYCLE_ICONS[family];
  const outputIcon: LucideIcon =
    operation === "cancel"
      ? X
      : operation === "cleanup" || operation === "clear" || operation === "recover"
        ? RotateCw
        : operation === "list" || operation === "status"
          ? Activity
          : icon;

  return {
    route,
    buildBody: noBody,
    routeFor,
    outputKind: code,
    formatText: recordFormatter(formatJobLifecycle),
    actionIcon: icon,
    outputIcon,
    structuredView: "job-lifecycle",
  };
}

const JOB_FAMILIES: JobFamily[] = ["crawl", "embed", "extract", "ingest"];
const JOB_OPERATIONS: JobOperation[] = ["list", "status", "cancel", "cleanup", "clear", "recover"];

function buildLifecycleRegistry(): Record<`${JobFamily}-${JobOperation}`, ActionBehavior> {
  const out = {} as Record<`${JobFamily}-${JobOperation}`, ActionBehavior>;
  for (const family of JOB_FAMILIES) {
    for (const operation of JOB_OPERATIONS) {
      out[`${family}-${operation}`] = lifecycleBehavior(family, operation);
    }
  }
  return out;
}

/** Exhaustive per-action behavior table. Keyed by the full subcommand union. */
export const ACTION_REGISTRY: Record<PaletteSubcommand, ActionBehavior> = {
  ...STATIC_REGISTRY,
  ...buildLifecycleRegistry(),
};

/** Behavior for a subcommand. Falls back to a generic code/JSON entry if the
 * subcommand string is not a known `PaletteSubcommand` (defensive — all callers
 * pass typed subcommands, so this only guards untyped string call sites). */
export function actionBehavior(subcommand: string): ActionBehavior {
  return ACTION_REGISTRY[subcommand as PaletteSubcommand] ?? FALLBACK_BEHAVIOR;
}

const FALLBACK_BEHAVIOR: ActionBehavior = {
  route: postRoute("/v1/unknown"),
  buildBody: noBody,
  outputKind: code,
  formatText: formatCompact,
  actionIcon: HelpCircle,
  outputIcon: HelpCircle,
  structuredView: null,
};

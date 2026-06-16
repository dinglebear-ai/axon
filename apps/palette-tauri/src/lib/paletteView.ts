import {
  ACTIONS,
  type PaletteAction,
  acceptsDirectUrl,
  actionInvokedBy,
} from "@/lib/actions";
import { helpAction, isHelpRequest } from "@/lib/actionHelp";
import { actionDisplayMeta } from "@/lib/actionMeta";

// Canonical homes for these helpers/types live elsewhere; re-exported here so
// existing importers of `@/lib/paletteView` keep working.
export { firstUrl, hostLabel } from "@/lib/url";
export type { RunState } from "@/lib/runState";

export type ParsedCommand = { invoked?: PaletteAction; search: string; arg: string };

// Delay before focusing the command input after a view transition, giving the
// overlay time to mount/animate before we steal focus.
const FOCUS_DELAY_MS = 30;

export function focusInput(select = false) {
  window.setTimeout(() => {
    const input = document.querySelector<HTMLInputElement>(".command-input");
    input?.focus();
    if (select) input?.select();
  }, FOCUS_DELAY_MS);
}

export function parseCommand(raw: string): ParsedCommand {
  const trimmed = raw.trimStart();
  const [token = ""] = trimmed.split(/\s+/);
  const rest = trimmed.slice(token.length).trimStart();
  const localHelpAction = helpAction();

  if (actionInvokedBy(localHelpAction, token)) {
    return { invoked: localHelpAction, search: token, arg: rest };
  }

  const invoked = ACTIONS.find((action) => actionInvokedBy(action, token));
  if (invoked && isHelpRequest(rest)) {
    return { invoked: localHelpAction, search: token, arg: invoked.subcommand };
  }

  if (invoked) {
    return { invoked, search: token, arg: rest };
  }

  return { search: trimmed, arg: "" };
}

export function argumentFor(
  action: PaletteAction,
  modeAction: PaletteAction | null,
  parsed: ParsedCommand,
  query: string,
): string {
  if (modeAction?.subcommand === action.subcommand) return query.trim();
  if (parsed.invoked?.subcommand === action.subcommand) return parsed.arg;
  if (looksLikeUrl(parsed.search) && acceptsDirectUrl(action)) return parsed.search;
  return parsed.search;
}

export function validationMessage(action: PaletteAction, argument: string): string {
  if (action.argMode === "none" || action.argMode === "optionalSingle") return "";
  return argument.trim() ? "" : "Argument required";
}

export function actionHint(action: PaletteAction, search: string): string {
  if (acceptsDirectUrl(action) && looksLikeUrl(search)) return "Run URL";
  if (action.argMode === "none") return "Run";
  return "Select";
}

export function actionArgumentLabel(action: PaletteAction): string {
  switch (action.argMode) {
    case "none":
      return "No input";
    case "optionalSingle":
      return "Optional input";
    case "single":
      return "Text input";
    case "split":
      return "Structured input";
  }
}

const CATEGORY_ORDER = ["Fetch & read", "Crawl & ingest", "Search & discover", "Reason", "Inspect", "Watch", "Jobs", "System", "Actions"];
const ACTION_ORDER = [
  "scrape",
  "map",
  "retrieve",
  "screenshot",
  "diff",
  "crawl",
  "ingest",
  "embed",
  "extract",
  "search",
  "research",
  "query",
  "sources",
  "domains",
  "ask",
  "summarize",
  "suggest",
  "evaluate",
  "status",
  "stats",
  "doctor",
  "brand",
  "endpoints",
  "dedupe",
];

export function sortActionsForDisplay(actions: PaletteAction[]): PaletteAction[] {
  return [...actions].sort((a, b) => {
    const metaA = actionDisplayMeta(a);
    const metaB = actionDisplayMeta(b);
    const categoryDelta = rank(CATEGORY_ORDER, metaA.category) - rank(CATEGORY_ORDER, metaB.category);
    if (categoryDelta) return categoryDelta;
    return rank(ACTION_ORDER, a.subcommand) - rank(ACTION_ORDER, b.subcommand);
  });
}

/**
 * Lower is a better match. Prefers an exact/prefix hit on the subcommand (what the
 * user is most likely typing) over an alias prefix, then a label prefix/word-start,
 * then a substring anywhere — so typing "cr" surfaces `crawl`, not `scrape`.
 */
function relevanceScore(action: PaletteAction, query: string): number {
  const sub = action.subcommand.toLowerCase();
  const label = action.label.toLowerCase();
  const aliases = action.aliases.map((alias) => alias.toLowerCase());
  if (sub === query || aliases.includes(query)) return 0;
  if (sub.startsWith(query)) return 1;
  if (aliases.some((alias) => alias.startsWith(query))) return 2;
  if (label.startsWith(query)) return 3;
  if (label.split(/\s+/).some((word) => word.startsWith(query))) return 4;
  if (sub.includes(query)) return 5;
  return 6;
}

/**
 * Rank matched actions by how well they match the query, falling back to the
 * canonical category/action order for ties (and for an empty query, which keeps the
 * grouped browse view). Use this for filtered search; `sortActionsForDisplay` is the
 * query-less browse ordering.
 */
export function sortActionsByRelevance(actions: PaletteAction[], query: string): PaletteAction[] {
  const q = query.trim().toLowerCase();
  if (!q) return sortActionsForDisplay(actions);
  return [...actions].sort((a, b) => {
    const scoreDelta = relevanceScore(a, q) - relevanceScore(b, q);
    if (scoreDelta) return scoreDelta;
    const categoryDelta =
      rank(CATEGORY_ORDER, actionDisplayMeta(a).category) - rank(CATEGORY_ORDER, actionDisplayMeta(b).category);
    if (categoryDelta) return categoryDelta;
    return rank(ACTION_ORDER, a.subcommand) - rank(ACTION_ORDER, b.subcommand);
  });
}

function rank(order: string[], value: string): number {
  const index = order.indexOf(value);
  return index === -1 ? order.length : index;
}

export function argumentPlaceholder(action: PaletteAction): string {
  const example = action.example.replace(new RegExp(`^${action.subcommand}\\s*`, "i"), "").trim();
  return example || action.description;
}

export function looksLikeUrl(value: string): boolean {
  const trimmed = value.trim();
  return /^https?:\/\//i.test(trimmed) || /^[a-z0-9-]+(\.[a-z0-9-]+)+(\/\S*)?$/i.test(trimmed);
}

export function asyncJobStart(payload: unknown): { jobId: string; status: string } | null {
  const result = recordField(payload, "result") ?? recordField(payload, "job") ?? payload;
  if (!isRecord(result)) return null;
  const jobId = stringField(result, "job_id") ?? stringField(result, "id");
  if (!jobId) return null;
  const rawStatus =
    stringField(result, "status") ??
    stringField(recordField(payload, "result") ?? {}, "status") ??
    stringField(payload, "disposition") ??
    "queued";
  if (/^(completed|failed|error)$/i.test(rawStatus)) return null;
  return { jobId, status: "queued" };
}

function stringField(value: unknown, key: string): string | undefined {
  return isRecord(value) && typeof value[key] === "string" ? value[key] : undefined;
}

function recordField(value: unknown, key: string): Record<string, unknown> | undefined {
  return isRecord(value) && isRecord(value[key]) ? value[key] : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

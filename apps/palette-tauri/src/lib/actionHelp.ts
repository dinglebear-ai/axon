import { ACTIONS, type PaletteAction, actionInvokedBy } from "@/lib/actions";
import { actionDisplayMeta } from "@/lib/actionMeta";
import type { HttpMethod, PaletteResult } from "@/lib/axonClient";

export interface ActionHelp {
  title: string;
  subcommand: string;
  aliases: string[];
  description: string;
  usage: string;
  category: string;
  route: { method: HttpMethod; path: string };
  output: string;
  async: boolean;
  parameters: string[];
  options: string[];
}

export interface HelpRunState {
  kind: "success";
  title: string;
  subtitle: string;
  text: string;
  outputKind: "markdown";
  result: PaletteResult;
}

const ASYNC_ACTIONS = new Set(["crawl", "embed", "extract", "ingest"]);

export function isAsyncAction(action: PaletteAction): boolean {
  return ASYNC_ACTIONS.has(action.subcommand);
}

const PARAMETER_DETAILS: Record<string, string[]> = {
  scrape: ["url from input", "collection from palette settings when configured"],
  crawl: ["urls from input", "collection from palette settings when configured"],
  ask: ["query from input", "collection from palette settings when configured", "explain=false", "diagnostics=false"],
  chat: ["message from input"],
  query: ["query from input", "limit from palette settings", "collection from palette settings when configured"],
  retrieve: ["url from input", "token_budget=6000", "collection from palette settings when configured"],
  search: ["query from input", "limit from palette settings"],
  research: ["query from input", "limit from palette settings"],
  map: ["url from input", "limit=100"],
  dedupe: ["collection from palette settings when configured"],
  screenshot: ["url from input", "full_page=true", "default backend viewport"],
};

const OPTION_DETAILS: Record<string, string[]> = {
  scrape: ["--collection is currently driven by palette settings", "--embed, --format, and --header are backend options not editable in the palette yet"],
  crawl: ["--collection is currently driven by palette settings", "--max-pages, --max-depth, --render-mode, --respect-robots, and --header are backend options not editable in the palette yet"],
  ask: ["--collection is currently driven by palette settings", "--explain and --diagnostics are fixed false in the current palette request"],
  query: ["--limit and --collection are currently driven by palette settings"],
  retrieve: ["--token-budget is fixed to 6000 in the current palette request", "--collection is currently driven by palette settings"],
  search: ["--limit is currently driven by palette settings"],
  research: ["--limit is currently driven by palette settings"],
  dedupe: ["--collection is currently driven by palette settings"],
  screenshot: ["Viewport options are backend-supported but not editable in the palette yet"],
};

export function isHelpRequest(value: string): boolean {
  return /^(?:help|--help|-h|\?)$/i.test(value.trim());
}

export function helpAction(): PaletteAction {
  const action = ACTIONS.find((candidate) => candidate.subcommand === "help");
  if (!action) throw new Error("missing local help action");
  return action;
}

export function findHelpTarget(value: string): PaletteAction | undefined {
  const token = value.trim().split(/\s+/)[0] ?? "";
  if (!token) return undefined;
  return ACTIONS.find((action) => action.subcommand !== "help" && actionInvokedBy(action, token));
}

export function buildActionHelp(action: PaletteAction): ActionHelp {
  const meta = actionDisplayMeta(action);
  return {
    title: action.label,
    subcommand: action.subcommand,
    aliases: action.aliases,
    description: action.description,
    usage: action.example,
    category: meta.category,
    route: { method: meta.method, path: meta.endpoint },
    output: meta.output,
    async: isAsyncAction(action),
    parameters: PARAMETER_DETAILS[action.subcommand] ?? (action.argMode === "none" ? ["none"] : ["input from command text"]),
    options: OPTION_DETAILS[action.subcommand] ?? ["No palette-specific options are exposed yet."],
  };
}

export function buildCatalogHelp(): ActionHelp[] {
  return ACTIONS.filter((action) => action.subcommand !== "help").map(buildActionHelp);
}

export function helpMarkdown(target?: PaletteAction, unknownTarget?: string): string {
  if (!target) {
    const groups = new Map<string, ActionHelp[]>();
    for (const item of buildCatalogHelp()) {
      const list = groups.get(item.category) ?? [];
      list.push(item);
      groups.set(item.category, list);
    }
    return [
      "# Axon Palette Help",
      "",
      unknownTarget ? `No matching action: \`${unknownTarget}\`` : "",
      "Use `help <action>`, `<action> help`, `<action> --help`, or the selected action `?` button.",
      "",
      ...[...groups.entries()].flatMap(([category, items]) => [
        `## ${category}`,
        "",
        ...items.map((item) => `- \`${item.subcommand}\` - ${item.description}`),
        "",
      ]),
    ].filter(Boolean).join("\n").trim();
  }

  const help = buildActionHelp(target);
  return [
    `# ${help.title}`,
    "",
    help.description,
    "",
    `Route: \`${help.route.method} ${help.route.path}\``,
    `Usage: \`${help.usage}\``,
    `Output: ${help.output}${help.async ? " (async job)" : ""}`,
    "",
    "## Parameters",
    ...help.parameters.map((param) => `- ${param}`),
    "",
    "## Options",
    ...help.options.map((option) => `- ${option}`),
    "",
    help.aliases.length ? `Aliases: ${help.aliases.map((alias) => `\`${alias}\``).join(", ")}` : "",
  ].filter(Boolean).join("\n").trim();
}

export function buildHelpPayload(
  target?: PaletteAction,
  unknownTarget?: string,
): { target?: ActionHelp; catalog?: ActionHelp[]; unknownTarget?: string } {
  return target ? { target: buildActionHelp(target) } : { catalog: buildCatalogHelp(), unknownTarget };
}

export function buildHelpRun(target?: PaletteAction, unknownTarget?: string): HelpRunState {
  const text = helpMarkdown(target, unknownTarget);
  const result: PaletteResult = {
    ok: true,
    status: 200,
    path: "palette://help",
    method: "GET",
    payload: buildHelpPayload(target, unknownTarget),
  };
  return {
    kind: "success",
    title: target ? `${target.label} help` : "Palette help",
    subtitle: target ? `${target.subcommand} help` : "help",
    text,
    outputKind: "markdown",
    result,
  };
}

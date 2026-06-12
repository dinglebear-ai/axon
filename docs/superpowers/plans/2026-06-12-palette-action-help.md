# Palette Action Help Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add first-class palette help so users can run `help`, `help scrape`, `scrape help`, or click a selected-action `?` button to see exactly what an action does, its route, accepted input, current request params, and available flags/options.

**Architecture:** Help is local to the Tauri palette in this phase and must never call Axon REST. Request/display metadata moves into a neutral `actionMeta.ts` module consumed by `paletteView.ts`, `actionHelp.ts`, and tests, avoiding circular imports and reducing route drift. Help renders as a compact structured result view with minimal metadata; editable options and future option taxonomy stay out of scope until option editing exists.

**Tech Stack:** React 19, TypeScript, Vite/Vitest, existing Aurora palette CSS, existing Axon palette action catalog.

---

## Reviewed Changes Locked In

- Add `apps/palette-tauri/src/lib/actionMeta.ts`; both `paletteView.ts` and `actionHelp.ts` consume it.
- Keep local help above backend/config guards in `useActionRunner.ts`.
- Make `buildActionRequest()` reject local actions defensively.
- Redesign `ActionList` rows before adding a `?`; no nested `<button>` inside another `<button>`.
- Add tests proving every help entry point avoids `executeAction` / `axon_http_request`.
- Keep structured help output, but remove `HelpValueSource`, `HelpOptionSupport`, and large future-option overlays.

## File Structure

- Modify: `apps/palette-tauri/src/lib/actions.ts`
  - Add the `help` action.
  - Add `"local"` to `PaletteAction.kind`.
- Create: `apps/palette-tauri/src/lib/actionMeta.ts`
  - Own `ActionDisplayMeta`, `ACTION_META`, `actionDisplayMeta()`, and action kind helpers.
  - Keep route/method metadata aligned with `buildActionRequest()`.
- Modify: `apps/palette-tauri/src/lib/paletteView.ts`
  - Import action display/kind helpers from `actionMeta.ts`.
  - Parse `help`, `help scrape`, `scrape help`, `fetch help`, `crawl --help`, and `?`.
- Create: `apps/palette-tauri/src/lib/actionHelp.ts`
  - Own `isHelpRequest()`, `findHelpTarget()`, `buildActionHelp()`, `buildHelpRun()`, and compact structured help payloads.
- Modify: `apps/palette-tauri/src/lib/axonClient.ts`
  - Reject `action.kind === "local"` in `buildActionRequest()`.
- Modify: `apps/palette-tauri/src/lib/useActionRunner.ts`
  - Handle help before checking `client`/`config`, before validation, and before request construction.
- Create: `apps/palette-tauri/src/components/palette/HelpResultView.tsx`
  - Render a clean structured help view from local payload.
- Modify: `apps/palette-tauri/src/components/palette/OperationResultView.tsx`
  - Route `palette://help` / `subcommand === "help"` to `HelpResultView`.
- Modify: `apps/palette-tauri/src/components/palette/ActionList.tsx`
  - Change row structure to a non-button wrapper with separate Run/Select and Help buttons.
- Modify: `apps/palette-tauri/src/App.tsx`
  - Add `showHelpFor(action?)` and wire selected-row plus command-bar help buttons.
- Modify: `apps/palette-tauri/src/styles.css`
  - Style compact help cards and safe row help affordances.
- Test: `apps/palette-tauri/src/lib/actionHelp.test.ts`
- Test: `apps/palette-tauri/src/lib/paletteView.test.ts`
- Test: `apps/palette-tauri/src/lib/axonClient.test.ts`
- Test: `apps/palette-tauri/src/lib/useActionRunner.test.tsx`
- Test: `apps/palette-tauri/src/components/palette/ActionList.test.tsx`
- Test: `apps/palette-tauri/src/components/palette/OperationResultView.test.tsx`

## Design Rules

- Help is local-only; no Axon backend route, MCP route, or `/v1/actions` route is added.
- `help`, `help <action>`, `<action> help`, `<action> --help`, `<action> -h`, and selected-action `?` must not call `executeAction()`.
- Help works when server config is missing, invalid, or still loading.
- No nested interactive controls. A help click must not submit or enter the action.
- Unknown targets render catalog help plus a visible “No matching action” note.
- Structured help output may use cards and tables, but long option names and paths must wrap.
- Do not display real tokens, custom header values, env values, bearer strings, or full config values in help.
- Do not add editable option controls in this feature.

---

### Task 1: Move Action Display Metadata Into `actionMeta.ts`

**Files:**
- Create: `apps/palette-tauri/src/lib/actionMeta.ts`
- Modify: `apps/palette-tauri/src/lib/paletteView.ts`
- Test: `apps/palette-tauri/src/lib/paletteView.test.ts`

- [ ] **Step 1: Write failing metadata tests**

Add to `apps/palette-tauri/src/lib/paletteView.test.ts`:

```ts
import { actionDisplayMeta, actionKindLabel, actionKindTone } from "./actionMeta";

it("exposes local help route metadata from actionMeta", () => {
  expect(actionDisplayMeta(action("help"))).toEqual({
    category: "System",
    endpoint: "palette://help",
    input: "action",
    output: "help",
    label: "Help",
    method: "GET",
  });
});

it("labels local actions without treating them like backend operations", () => {
  expect(actionKindLabel(action("help"))).toBe("Local");
  expect(actionKindTone(action("help"))).toBe("info");
});

it("keeps retrieve route metadata aligned with the actual palette request", () => {
  expect(actionDisplayMeta(action("retrieve"))).toMatchObject({
    endpoint: "/v1/retrieve",
    method: "POST",
  });
});
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cd /home/jmagar/workspace/axon/apps/palette-tauri
pnpm vitest run src/lib/paletteView.test.ts
```

Expected: FAIL because `actionMeta.ts` does not exist and `help` is not yet an action.

- [ ] **Step 3: Add the local help action**

Modify `apps/palette-tauri/src/lib/actions.ts`:

```ts
export interface PaletteAction {
  label: string;
  subcommand: string;
  kind?: "operation" | "job" | "admin" | "discovery" | "local";
  argMode: ArgMode;
  aliases: string[];
  description: string;
  example: string;
  tone: "info" | "success" | "warn" | "neutral" | "rose" | "violet";
}
```

Insert this as the first `ACTIONS` item:

```ts
{
  label: "Help",
  subcommand: "help",
  kind: "local",
  argMode: "optionalSingle",
  aliases: ["help", "?", "--help", "-h"],
  description: "Show command help, usage, current request params, and available options.",
  example: "help scrape",
  tone: "info",
},
```

- [ ] **Step 4: Create neutral metadata module**

Create `apps/palette-tauri/src/lib/actionMeta.ts`:

```ts
import type { PaletteAction } from "@/lib/actions";
import type { HttpMethod } from "@/lib/axonClient";

export type ActionDisplayMeta = {
  category: string;
  endpoint: string;
  input: string;
  output: string;
  label: string;
  method: Extract<HttpMethod, "GET" | "POST">;
};

const ACTION_META: Record<string, ActionDisplayMeta> = {
  help: { category: "System", endpoint: "palette://help", input: "action", output: "help", label: "Help", method: "GET" },
  scrape: { category: "Fetch & read", endpoint: "/v1/scrape", input: "one URL", output: "content", label: "Scrape", method: "POST" },
  map: { category: "Fetch & read", endpoint: "/v1/map", input: "one URL", output: "links", label: "Map", method: "POST" },
  retrieve: { category: "Fetch & read", endpoint: "/v1/retrieve", input: "URL", output: "chunks", label: "Retrieve", method: "POST" },
  screenshot: { category: "Fetch & read", endpoint: "/v1/screenshot", input: "URL", output: "PNG", label: "Screenshot", method: "POST" },
  diff: { category: "Fetch & read", endpoint: "/v1/diff", input: "two URLs", output: "changes", label: "Diff", method: "POST" },
  crawl: { category: "Crawl & ingest", endpoint: "/v1/crawl", input: "start URL", output: "job", label: "Crawl", method: "POST" },
  ingest: { category: "Crawl & ingest", endpoint: "/v1/ingest", input: "target", output: "job", label: "Ingest", method: "POST" },
  embed: { category: "Crawl & ingest", endpoint: "/v1/embed", input: "input", output: "vectors", label: "Embed", method: "POST" },
  extract: { category: "Crawl & ingest", endpoint: "/v1/extract", input: "URLs", output: "data", label: "Extract", method: "POST" },
  "ingest-sessions-prepared": { category: "Crawl & ingest", endpoint: "/v1/ingest/sessions/prepared", input: "JSON", output: "job", label: "Prepared sessions", method: "POST" },
  search: { category: "Search & discover", endpoint: "/v1/search", input: "query", output: "results", label: "Search", method: "POST" },
  research: { category: "Search & discover", endpoint: "/v1/research", input: "query", output: "brief", label: "Research", method: "POST" },
  query: { category: "Search & discover", endpoint: "/v1/query", input: "query", output: "chunks", label: "Query", method: "POST" },
  sources: { category: "Search & discover", endpoint: "/v1/sources", input: "none", output: "URLs", label: "Sources", method: "GET" },
  domains: { category: "Search & discover", endpoint: "/v1/domains", input: "none", output: "domains", label: "Domains", method: "GET" },
  ask: { category: "Reason", endpoint: "/v1/ask", input: "question", output: "answer", label: "Ask", method: "POST" },
  chat: { category: "Reason", endpoint: "/v1/chat", input: "message", output: "answer", label: "Chat", method: "POST" },
  summarize: { category: "Reason", endpoint: "/v1/summarize", input: "URLs", output: "summary", label: "Summarize", method: "POST" },
  suggest: { category: "Reason", endpoint: "/v1/suggest", input: "focus", output: "URLs", label: "Suggest", method: "POST" },
  evaluate: { category: "Reason", endpoint: "/v1/evaluate", input: "question", output: "score", label: "Evaluate", method: "POST" },
  status: { category: "System", endpoint: "/v1/status", input: "none", output: "jobs", label: "Status", method: "GET" },
  stats: { category: "System", endpoint: "/v1/stats", input: "none", output: "stats", label: "Stats", method: "GET" },
  doctor: { category: "System", endpoint: "/v1/doctor", input: "none", output: "health", label: "Doctor", method: "GET" },
  endpoints: { category: "Inspect", endpoint: "/v1/endpoints", input: "URL", output: "endpoints", label: "Endpoints", method: "POST" },
  brand: { category: "Inspect", endpoint: "/v1/brand", input: "URL", output: "brand", label: "Brand", method: "POST" },
  dedupe: { category: "System", endpoint: "/v1/dedupe", input: "collection", output: "report", label: "Dedupe", method: "POST" },
  "watch-list": { category: "Watch", endpoint: "/v1/watch", input: "none", output: "watches", label: "Watch list", method: "GET" },
  "watch-create": { category: "Watch", endpoint: "/v1/watch", input: "URL", output: "watch", label: "Watch create", method: "POST" },
  "watch-run": { category: "Watch", endpoint: "/v1/watch/{id}/run", input: "watch id", output: "run", label: "Watch run", method: "POST" },
};

export function actionDisplayMeta(action: PaletteAction): ActionDisplayMeta {
  return ACTION_META[action.subcommand] ?? {
    category: action.kind === "local" ? "System" : "Other",
    endpoint: action.kind === "local" ? `palette://${action.subcommand}` : `/v1/${action.subcommand}`,
    input: action.argMode === "none" ? "none" : "input",
    output: action.kind === "local" ? "local" : "result",
    label: action.label,
    method: action.kind === "local" ? "GET" : "POST",
  };
}

export function actionKindLabel(action: PaletteAction): string {
  switch (action.kind) {
    case "admin":
      return "Admin";
    case "discovery":
      return "Discovery";
    case "job":
      return "Job";
    case "local":
      return "Local";
    case "operation":
    default:
      return "Operation";
  }
}

export function actionKindTone(action: PaletteAction): "info" | "success" | "warn" | "neutral" | "rose" | "violet" {
  switch (action.kind) {
    case "admin":
      return "warn";
    case "discovery":
      return "neutral";
    case "job":
      return "violet";
    case "local":
      return "info";
    case "operation":
    default:
      return "info";
  }
}
```

- [ ] **Step 5: Remove metadata from `paletteView.ts`**

In `apps/palette-tauri/src/lib/paletteView.ts`:

```ts
import { actionDisplayMeta, actionKindLabel, actionKindTone, type ActionDisplayMeta } from "@/lib/actionMeta";
```

Delete the local `ActionDisplayMeta` type, `DISPLAY_META`, `actionDisplayMeta()`, `actionKindLabel()`, and `actionKindTone()` definitions from `paletteView.ts`. Leave existing exports that are still owned by `paletteView.ts`, including `parseCommand`, `argumentFor`, `validationMessage`, `actionHint`, `actionArgumentLabel`, and sort helpers.

- [ ] **Step 6: Run metadata tests**

Run:

```bash
cd /home/jmagar/workspace/axon/apps/palette-tauri
pnpm vitest run src/lib/paletteView.test.ts
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
cd /home/jmagar/workspace/axon
git add apps/palette-tauri/src/lib/actions.ts apps/palette-tauri/src/lib/actionMeta.ts apps/palette-tauri/src/lib/paletteView.ts apps/palette-tauri/src/lib/paletteView.test.ts
git commit -m "feat(palette): centralize action metadata"
```

---

### Task 2: Add Local Help Metadata and Parsing

**Files:**
- Create: `apps/palette-tauri/src/lib/actionHelp.ts`
- Modify: `apps/palette-tauri/src/lib/paletteView.ts`
- Test: `apps/palette-tauri/src/lib/actionHelp.test.ts`
- Test: `apps/palette-tauri/src/lib/paletteView.test.ts`

- [ ] **Step 1: Write failing help metadata tests**

Create `apps/palette-tauri/src/lib/actionHelp.test.ts`:

```ts
import { describe, expect, it } from "vitest";

import { ACTIONS } from "./actions";
import { buildActionHelp, buildHelpRun, findHelpTarget, isHelpRequest } from "./actionHelp";

function action(subcommand: string) {
  const found = ACTIONS.find((candidate) => candidate.subcommand === subcommand);
  if (!found) throw new Error(`missing action ${subcommand}`);
  return found;
}

describe("action help", () => {
  it("recognizes only exact help tokens", () => {
    expect(isHelpRequest("help")).toBe(true);
    expect(isHelpRequest("--help")).toBe(true);
    expect(isHelpRequest("-h")).toBe(true);
    expect(isHelpRequest("?")).toBe(true);
    expect(isHelpRequest("help me scrape")).toBe(false);
  });

  it("finds targets by subcommand and alias", () => {
    expect(findHelpTarget("scrape")?.subcommand).toBe("scrape");
    expect(findHelpTarget("fetch")?.subcommand).toBe("scrape");
    expect(findHelpTarget("crawl")?.subcommand).toBe("crawl");
    expect(findHelpTarget("missing")).toBeUndefined();
  });

  it("builds target help from neutral action metadata", () => {
    const help = buildActionHelp(action("scrape"));
    expect(help.title).toBe("Scrape URL");
    expect(help.route).toEqual({ method: "POST", path: "/v1/scrape" });
    expect(help.usage).toBe("scrape https://docs.rs/serde");
    expect(help.parameters).toEqual(expect.arrayContaining(["url from input", "collection from palette settings when configured"]));
  });

  it("builds catalog run state with structured local payload", () => {
    const run = buildHelpRun();
    expect(run.kind).toBe("success");
    expect(run.result.path).toBe("palette://help");
    expect(run.outputKind).toBe("markdown");
    expect(run.text).toContain("# Axon Palette Help");
    expect(run.text).toContain("`scrape`");
  });

  it("builds unknown-target help with a visible note", () => {
    const run = buildHelpRun(undefined, "nope");
    expect(run.text).toContain("No matching action: `nope`");
  });
});
```

- [ ] **Step 2: Write failing parser tests**

Add to `apps/palette-tauri/src/lib/paletteView.test.ts`:

```ts
it("parses bare help as the local help action", () => {
  expect(parseCommand("help")).toMatchObject({ invoked: action("help"), search: "help", arg: "" });
});

it("parses help followed by an action target", () => {
  expect(parseCommand("help scrape")).toMatchObject({ invoked: action("help"), search: "help", arg: "scrape" });
});

it("parses action help without invoking the backend action", () => {
  expect(parseCommand("scrape help")).toMatchObject({ invoked: action("help"), search: "scrape", arg: "scrape" });
  expect(parseCommand("fetch help")).toMatchObject({ invoked: action("help"), search: "fetch", arg: "scrape" });
  expect(parseCommand("crawl --help")).toMatchObject({ invoked: action("help"), search: "crawl", arg: "crawl" });
  expect(parseCommand("query -h")).toMatchObject({ invoked: action("help"), search: "query", arg: "query" });
});

it("leaves non-command help text searchable", () => {
  expect(parseCommand("help me debug this")).toMatchObject({ invoked: action("help"), search: "help", arg: "me debug this" });
});
```

- [ ] **Step 3: Run help tests to verify failure**

Run:

```bash
cd /home/jmagar/workspace/axon/apps/palette-tauri
pnpm vitest run src/lib/actionHelp.test.ts src/lib/paletteView.test.ts
```

Expected: FAIL because `actionHelp.ts` does not exist and `parseCommand()` is not help-aware.

- [ ] **Step 4: Implement minimal help metadata**

Create `apps/palette-tauri/src/lib/actionHelp.ts`:

```ts
import { ACTIONS, type PaletteAction, actionInvokedBy } from "@/lib/actions";
import { actionDisplayMeta } from "@/lib/actionMeta";
import type { PaletteResult } from "@/lib/axonClient";
import type { RunState } from "@/lib/runState";

export interface ActionHelp {
  title: string;
  subcommand: string;
  aliases: string[];
  description: string;
  usage: string;
  category: string;
  route: { method: "GET" | "POST"; path: string };
  output: string;
  async: boolean;
  parameters: string[];
  options: string[];
}

const ASYNC_ACTIONS = new Set(["crawl", "embed", "extract", "ingest", "ingest-sessions-prepared"]);

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
};

const OPTION_DETAILS: Record<string, string[]> = {
  scrape: ["--collection is currently driven by palette settings", "--embed, --format, and --header are backend options not editable in the palette yet"],
  crawl: ["--collection is currently driven by palette settings", "--max-pages, --max-depth, --render-mode, --respect-robots, and --header are backend options not editable in the palette yet"],
  ask: ["--collection is currently driven by palette settings", "--explain and --diagnostics are fixed false in the current palette request"],
  query: ["--limit and --collection are currently driven by palette settings"],
  retrieve: ["--token-budget is fixed to 6000 in the current palette request", "--collection is currently driven by palette settings"],
  search: ["--limit is currently driven by palette settings"],
  research: ["--limit is currently driven by palette settings"],
};

export function isHelpRequest(value: string): boolean {
  return /^(?:help|--help|-h|\?)$/i.test(value.trim());
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
    async: ASYNC_ACTIONS.has(action.subcommand) || action.kind === "job",
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
      unknownTarget ? "" : "",
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

export function buildHelpPayload(target?: PaletteAction, unknownTarget?: string): { target?: ActionHelp; catalog?: ActionHelp[]; unknownTarget?: string } {
  return target ? { target: buildActionHelp(target) } : { catalog: buildCatalogHelp(), unknownTarget };
}

export function buildHelpRun(target?: PaletteAction, unknownTarget?: string): RunState {
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
```

- [ ] **Step 5: Implement help-aware parsing**

Modify `apps/palette-tauri/src/lib/paletteView.ts`:

```ts
import { findHelpTarget, isHelpRequest } from "@/lib/actionHelp";
```

Replace `parseCommand()` with:

```ts
export function parseCommand(raw: string): ParsedCommand {
  const trimmed = raw.trimStart();
  const [token = ""] = trimmed.split(/\s+/);
  const rest = trimmed.slice(token.length).trimStart();
  const helpAction = ACTIONS.find((action) => action.subcommand === "help");

  if (helpAction && actionInvokedBy(helpAction, token)) {
    return { invoked: helpAction, search: token, arg: rest };
  }

  const invoked = ACTIONS.find((action) => actionInvokedBy(action, token));
  if (helpAction && invoked && isHelpRequest(rest)) {
    return { invoked: helpAction, search: token, arg: invoked.subcommand };
  }

  if (invoked) {
    return { invoked, search: token, arg: rest };
  }

  const helpTarget = findHelpTarget(trimmed);
  if (helpAction && helpTarget) {
    return { invoked: helpAction, search: helpTarget.subcommand, arg: helpTarget.subcommand };
  }

  return { search: trimmed, arg: "" };
}
```

- [ ] **Step 6: Run help parsing tests**

Run:

```bash
cd /home/jmagar/workspace/axon/apps/palette-tauri
pnpm vitest run src/lib/actionHelp.test.ts src/lib/paletteView.test.ts
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
cd /home/jmagar/workspace/axon
git add apps/palette-tauri/src/lib/actionHelp.ts apps/palette-tauri/src/lib/actionHelp.test.ts apps/palette-tauri/src/lib/paletteView.ts apps/palette-tauri/src/lib/paletteView.test.ts
git commit -m "feat(palette): parse local action help"
```

---

### Task 3: Keep Help Local Before Backend Guards

**Files:**
- Modify: `apps/palette-tauri/src/lib/axonClient.ts`
- Modify: `apps/palette-tauri/src/lib/useActionRunner.ts`
- Test: `apps/palette-tauri/src/lib/axonClient.test.ts`
- Test: `apps/palette-tauri/src/lib/useActionRunner.test.tsx`

- [ ] **Step 1: Add defensive client test**

Add to `apps/palette-tauri/src/lib/axonClient.test.ts`:

```ts
it("rejects local actions before request construction", () => {
  const client = createAxonClient(config);
  expect(() => buildActionRequest(client, action("help"), "scrape", config)).toThrow("Local action help cannot be sent to Axon REST");
});
```

- [ ] **Step 2: Add no-REST runner tests**

Create `apps/palette-tauri/src/lib/useActionRunner.test.tsx`:

```tsx
import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useState } from "react";

import type { HistoryItem } from "@/components/palette/HistoryPanel";
import { ACTIONS, type PaletteAction } from "@/lib/actions";
import type { Client, PaletteConfig } from "@/lib/axonClient";
import { parseCommand } from "@/lib/paletteView";
import type { RunState } from "@/lib/runState";
import { useActionRunner } from "@/lib/useActionRunner";

function action(subcommand: string) {
  const found = ACTIONS.find((candidate) => candidate.subcommand === subcommand);
  if (!found) throw new Error(`missing action ${subcommand}`);
  return found;
}

const config: PaletteConfig = {
  serverUrl: "http://127.0.0.1:9999",
  token: null,
  shortcut: "Ctrl+Space",
  collection: "axon",
  resultLimit: 10,
  theme: "dark",
  hideOnBlur: false,
};

const client: Client = { baseUrl: "http://127.0.0.1:9999", headers: {} };

function setup(query: string, overrides: { client?: Client | null; config?: PaletteConfig | null } = {}) {
  const parsed = parseCommand(query);
  const wrapper = renderHook(() => {
    const [run, setRun] = useState<RunState>({ kind: "idle" });
    const [history, setHistory] = useState<HistoryItem[]>([]);
    const [modeAction, setModeAction] = useState<PaletteAction | null>(null);
    const [input, setQuery] = useState(query);
    const [browseOpen, setBrowseOpen] = useState(false);
    const runner = useActionRunner({
      client: overrides.client === undefined ? client : overrides.client,
      config: overrides.config === undefined ? config : overrides.config,
      run,
      setRun,
      setHistory,
      setModeAction,
      setQuery,
      setBrowseOpen,
      modeAction,
      parsed,
      query: input,
    });
    return { ...runner, run, history, parsed };
  });
  return wrapper;
}

describe("useActionRunner local help", () => {
  it.each([
    ["help", "help"],
    ["help scrape", "help"],
    ["scrape help", "help"],
    ["fetch help", "help"],
    ["crawl --help", "help"],
    ["?", "help"],
  ])("handles %s without requiring a backend client", async (query, subcommand) => {
    const rendered = setup(query, { client: null, config: null });
    await act(async () => {
      await rendered.result.current.submit(action(subcommand));
    });
    expect(rendered.result.current.run.kind).toBe("success");
    expect("result" in rendered.result.current.run ? rendered.result.current.run.result.path : "").toBe("palette://help");
  });
});
```

- [ ] **Step 3: Run tests to verify failure**

Run:

```bash
cd /home/jmagar/workspace/axon/apps/palette-tauri
pnpm vitest run src/lib/axonClient.test.ts src/lib/useActionRunner.test.tsx
```

Expected: FAIL because local request rejection and runner interception are not implemented.

- [ ] **Step 4: Reject local actions in request construction**

Modify `apps/palette-tauri/src/lib/axonClient.ts`:

```ts
export function buildActionRequest(
  client: Client,
  action: PaletteAction,
  arg: string,
  config: PaletteConfig,
): PaletteHttpRequest {
  if (action.kind === "local") {
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
```

- [ ] **Step 5: Intercept help before backend/config guards**

Modify `apps/palette-tauri/src/lib/useActionRunner.ts` imports:

```ts
import { buildHelpRun, findHelpTarget, isHelpRequest } from "@/lib/actionHelp";
```

At the top of `submit()` before `if (!client || !config || !action || run.kind === "running" || run.kind === "streaming") return;`, add:

```ts
    if (!action || run.kind === "running" || run.kind === "streaming") return;
    const rawArgument = argumentOverride ?? argumentFor(action, modeAction, parsed, query);
    if (action.subcommand === "help" || isHelpRequest(rawArgument)) {
      const targetToken = action.subcommand === "help" ? rawArgument : action.subcommand;
      const target = findHelpTarget(targetToken);
      const unknownTarget = action.subcommand === "help" && targetToken.trim() && !target ? targetToken.trim() : undefined;
      const helpRun = buildHelpRun(target, unknownTarget);
      setRun(helpRun);
      setModeAction(action);
      setQuery(action.subcommand === "help" ? rawArgument.trim() : target?.subcommand ?? "");
      setBrowseOpen(false);
      pushHistory(action, target?.subcommand ?? unknownTarget ?? "catalog", 200, helpRun.text, "markdown");
      return;
    }

    if (!client || !config) return;
```

Then change the later argument declaration to reuse the raw argument:

```ts
    const argument = normalizeSubmitArgument(action, rawArgument);
```

- [ ] **Step 6: Run local-only tests**

Run:

```bash
cd /home/jmagar/workspace/axon/apps/palette-tauri
pnpm vitest run src/lib/axonClient.test.ts src/lib/useActionRunner.test.tsx
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
cd /home/jmagar/workspace/axon
git add apps/palette-tauri/src/lib/axonClient.ts apps/palette-tauri/src/lib/axonClient.test.ts apps/palette-tauri/src/lib/useActionRunner.ts apps/palette-tauri/src/lib/useActionRunner.test.tsx
git commit -m "fix(palette): keep help local"
```

---

### Task 4: Render Structured Help Output

**Files:**
- Create: `apps/palette-tauri/src/components/palette/HelpResultView.tsx`
- Modify: `apps/palette-tauri/src/components/palette/OperationResultView.tsx`
- Modify: `apps/palette-tauri/src/styles.css`
- Test: `apps/palette-tauri/src/components/palette/OperationResultView.test.tsx`

- [ ] **Step 1: Write failing structured render test**

Add to `apps/palette-tauri/src/components/palette/OperationResultView.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { OperationResultView } from "@/components/palette/OperationResultView";
import { ACTIONS } from "@/lib/actions";
import { buildHelpRun } from "@/lib/actionHelp";

function action(subcommand: string) {
  const found = ACTIONS.find((candidate) => candidate.subcommand === subcommand);
  if (!found) throw new Error(`missing action ${subcommand}`);
  return found;
}

it("renders action help as a structured help view", () => {
  const run = buildHelpRun(action("scrape"));
  render(<OperationResultView action={action("help")} run={run} outputKind="markdown" />);
  expect(screen.getByRole("heading", { name: "Scrape URL" })).toBeInTheDocument();
  expect(screen.getByText("POST /v1/scrape")).toBeInTheDocument();
  expect(screen.getByText("Parameters")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run render test to verify failure**

Run:

```bash
cd /home/jmagar/workspace/axon/apps/palette-tauri
pnpm vitest run src/components/palette/OperationResultView.test.tsx
```

Expected: FAIL because `HelpResultView` is not wired.

- [ ] **Step 3: Add `HelpResultView`**

Create `apps/palette-tauri/src/components/palette/HelpResultView.tsx`:

```tsx
import type { ActionHelp } from "@/lib/actionHelp";

function isActionHelp(value: unknown): value is ActionHelp {
  if (!value || typeof value !== "object") return false;
  const item = value as Record<string, unknown>;
  const route = item.route as Record<string, unknown> | undefined;
  return (
    typeof item.title === "string" &&
    typeof item.subcommand === "string" &&
    route !== undefined &&
    (route.method === "GET" || route.method === "POST") &&
    typeof route.path === "string" &&
    Array.isArray(item.parameters) &&
    Array.isArray(item.options)
  );
}

function payloadRecord(payload: unknown): Record<string, unknown> {
  return payload && typeof payload === "object" ? (payload as Record<string, unknown>) : {};
}

export function HelpResultView({ payload, fallbackText }: { payload: unknown; fallbackText: string }) {
  const body = payloadRecord(payload);
  const target = isActionHelp(body.target) ? body.target : undefined;
  const catalog = Array.isArray(body.catalog) ? body.catalog.filter(isActionHelp) : [];
  const unknownTarget = typeof body.unknownTarget === "string" ? body.unknownTarget : "";

  if (!target && catalog.length === 0) {
    return <pre className="result-code">{fallbackText}</pre>;
  }

  if (target) {
    return (
      <div className="help-result">
        <header className="help-header">
          <span className="help-route">{target.route.method} {target.route.path}</span>
          <h2>{target.title}</h2>
          <p>{target.description}</p>
        </header>
        <section className="help-section">
          <h3>Usage</h3>
          <code>{target.usage}</code>
        </section>
        <section className="help-section">
          <h3>Parameters</h3>
          <ul>{target.parameters.map((item) => <li key={item}>{item}</li>)}</ul>
        </section>
        <section className="help-section">
          <h3>Options</h3>
          <ul>{target.options.map((item) => <li key={item}>{item}</li>)}</ul>
        </section>
        {target.aliases.length > 0 && (
          <section className="help-section help-aliases">
            <h3>Aliases</h3>
            <div>{target.aliases.map((alias) => <code key={alias}>{alias}</code>)}</div>
          </section>
        )}
      </div>
    );
  }

  const groups = new Map<string, ActionHelp[]>();
  for (const item of catalog) {
    const list = groups.get(item.category) ?? [];
    list.push(item);
    groups.set(item.category, list);
  }

  return (
    <div className="help-result">
      <header className="help-header">
        <span className="help-route">palette://help</span>
        <h2>Axon Palette Help</h2>
        <p>Use help &lt;action&gt;, &lt;action&gt; help, or the selected action question mark.</p>
        {unknownTarget ? <p className="help-warning">No matching action: <code>{unknownTarget}</code></p> : null}
      </header>
      {[...groups.entries()].map(([category, items]) => (
        <section className="help-section" key={category}>
          <h3>{category}</h3>
          <div className="help-catalog">
            {items.map((item) => (
              <article className="help-catalog-item" key={item.subcommand}>
                <code>{item.subcommand}</code>
                <span>{item.description}</span>
              </article>
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}
```

- [ ] **Step 4: Route help in `OperationResultView`**

In `apps/palette-tauri/src/components/palette/OperationResultView.tsx`, import:

```tsx
import { HelpResultView } from "@/components/palette/HelpResultView";
```

Before other structured operation branches, add:

```tsx
  if (run.result.path === "palette://help" || action.subcommand === "help") {
    return <HelpResultView payload={run.result.payload} fallbackText={run.text} />;
  }
```

- [ ] **Step 5: Add compact help styles**

Add to `apps/palette-tauri/src/styles.css`:

```css
.help-result {
  display: grid;
  gap: 14px;
  color: var(--text);
}

.help-header,
.help-section {
  border: 1px solid color-mix(in srgb, var(--border) 78%, transparent);
  background: color-mix(in srgb, var(--surface) 92%, transparent);
  border-radius: 8px;
  padding: 14px 16px;
}

.help-header h2,
.help-section h3 {
  margin: 0;
  line-height: 1.2;
}

.help-header p {
  margin: 8px 0 0;
  color: var(--muted);
}

.help-route {
  display: inline-flex;
  max-width: 100%;
  overflow-wrap: anywhere;
  color: var(--accent);
  font-family: var(--font-mono);
  font-size: 12px;
  margin-bottom: 8px;
}

.help-section code,
.help-catalog-item code {
  white-space: normal;
  overflow-wrap: anywhere;
}

.help-section ul {
  margin: 10px 0 0;
  padding-left: 20px;
}

.help-section li {
  margin: 6px 0;
  overflow-wrap: anywhere;
}

.help-aliases div,
.help-catalog {
  display: grid;
  gap: 8px;
}

.help-aliases div {
  grid-template-columns: repeat(auto-fit, minmax(72px, max-content));
}

.help-catalog-item {
  display: grid;
  grid-template-columns: minmax(92px, 150px) minmax(0, 1fr);
  gap: 12px;
  align-items: start;
}

.help-warning {
  color: var(--warning);
}

@media (max-width: 760px) {
  .help-catalog-item {
    grid-template-columns: 1fr;
    gap: 4px;
  }
}
```

- [ ] **Step 6: Run render test**

Run:

```bash
cd /home/jmagar/workspace/axon/apps/palette-tauri
pnpm vitest run src/components/palette/OperationResultView.test.tsx
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
cd /home/jmagar/workspace/axon
git add apps/palette-tauri/src/components/palette/HelpResultView.tsx apps/palette-tauri/src/components/palette/OperationResultView.tsx apps/palette-tauri/src/components/palette/OperationResultView.test.tsx apps/palette-tauri/src/styles.css
git commit -m "feat(palette): render structured help"
```

---

### Task 5: Add Safe `?` Affordances

**Files:**
- Modify: `apps/palette-tauri/src/components/palette/ActionList.tsx`
- Modify: `apps/palette-tauri/src/App.tsx`
- Modify: `apps/palette-tauri/src/styles.css`
- Test: `apps/palette-tauri/src/components/palette/ActionList.test.tsx`

- [ ] **Step 1: Write failing row help test**

Create `apps/palette-tauri/src/components/palette/ActionList.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useState } from "react";

import { ActionList } from "@/components/palette/ActionList";
import { ACTIONS } from "@/lib/actions";
import { parseCommand } from "@/lib/paletteView";

const onSubmit = vi.fn();
const onEnterMode = vi.fn();
const onHelp = vi.fn();

function Harness() {
  const [selected, setSelected] = useState(0);
  return (
    <ActionList
      filtered={ACTIONS.slice(0, 3)}
      selected={selected}
      setSelected={setSelected}
      parsed={parseCommand("")}
      onSubmit={onSubmit}
      onEnterMode={onEnterMode}
      onHelp={onHelp}
    />
  );
}

it("opens selected-row help without submitting or entering action mode", () => {
  onSubmit.mockClear();
  onEnterMode.mockClear();
  onHelp.mockClear();
  render(<Harness />);
  fireEvent.click(screen.getByRole("button", { name: "Help for Help" }));
  expect(onHelp).toHaveBeenCalledTimes(1);
  expect(onSubmit).not.toHaveBeenCalled();
  expect(onEnterMode).not.toHaveBeenCalled();
});
```

- [ ] **Step 2: Run row test to verify failure**

Run:

```bash
cd /home/jmagar/workspace/axon/apps/palette-tauri
pnpm vitest run src/components/palette/ActionList.test.tsx
```

Expected: FAIL because `onHelp` is not a prop.

- [ ] **Step 3: Redesign `ActionList` rows**

Modify `apps/palette-tauri/src/components/palette/ActionList.tsx` props:

```ts
interface ActionListProps {
  filtered: PaletteAction[];
  selected: number;
  setSelected: Dispatch<SetStateAction<number>>;
  parsed: ParsedCommand;
  onSubmit: (action: PaletteAction) => void;
  onEnterMode: (action: PaletteAction) => void;
  onHelp: (action: PaletteAction) => void;
}
```

Change the component signature:

```ts
export function ActionList({ filtered, selected, setSelected, parsed, onSubmit, onEnterMode, onHelp }: ActionListProps) {
```

Replace the row `<button className=...>` with this non-button wrapper and separate buttons:

```tsx
<div className={selectedRow ? "action-row action-row-selected" : "action-row"}>
  <button
    className="action-row-main"
    type="button"
    onClick={() => {
      setSelected(index);
      if (parsed.invoked) {
        onSubmit(action);
      } else if (action.argMode === "none") {
        onSubmit(action);
      } else if (acceptsDirectUrl(action) && looksLikeUrl(parsed.search)) {
        onSubmit(action);
      } else {
        onEnterMode(action);
      }
    }}
  >
    <ActionIcon action={action} selected={selectedRow} />
    <span className="action-main">
      <span className="action-title-line">
        <span className="action-label">{meta.label}</span>
        <span className="action-method">{meta.method}</span>
        <span className="action-endpoint">{meta.endpoint}</span>
        {action.subcommand === "crawl" || action.subcommand === "ingest" || action.subcommand === "embed" || action.subcommand === "extract" ? (
          <span className="action-async">ASYNC</span>
        ) : null}
      </span>
      <span className="action-description">{action.description}</span>
    </span>
  </button>
  <span className="action-meta">
    {selectedRow ? (
      <>
        <button className="action-help-button" type="button" onClick={() => onHelp(action)} aria-label={`Help for ${action.label}`} title={`Help for ${action.label}`}>
          ?
        </button>
        <span className="action-run-pill">Run <kbd>↵</kbd></span>
      </>
    ) : (
      <kbd>{action.subcommand}</kbd>
    )}
  </span>
</div>
```

- [ ] **Step 4: Add App help handler and command-bar button**

Modify `apps/palette-tauri/src/App.tsx` imports:

```ts
import { HelpCircle } from "lucide-react";
import { buildHelpRun } from "@/lib/actionHelp";
```

Add beside `enterActionMode()`:

```ts
  function showHelpFor(action?: PaletteAction) {
    const helpRun = buildHelpRun(action);
    const helpAction = ACTIONS.find((candidate) => candidate.subcommand === "help") ?? action ?? null;
    setModeAction(helpAction);
    setQuery(action?.subcommand ?? "");
    setRun(helpRun);
    setHistory((items) => [
      {
        action: helpAction ?? ACTIONS[0],
        target: action?.subcommand ?? "catalog",
        status: 200,
        text: helpRun.text,
        outputKind: "markdown",
        when: "just now",
      },
      ...items,
    ].slice(0, 18));
    setHistoryOpen(false);
    setSettingsOpen(false);
    setBrowseOpen(false);
  }
```

Pass it to `ActionList`:

```tsx
onHelp={showHelpFor}
```

Add a command-bar help button before submit:

```tsx
        <button
          className="command-help"
          type="button"
          onClick={() => showHelpFor(active)}
          disabled={!active || run.kind === "running" || run.kind === "streaming"}
          aria-label={active ? `Help for ${active.label}` : "Help"}
          title={active ? `Help for ${active.label}` : "Help"}
        >
          <HelpCircle size={15} />
        </button>
```

- [ ] **Step 5: Add safe row styles**

Add to `apps/palette-tauri/src/styles.css`:

```css
.action-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: stretch;
}

.action-row-main {
  min-width: 0;
  border: 0;
  background: transparent;
  color: inherit;
  text-align: left;
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 12px;
  align-items: center;
  padding: 10px 0 10px 10px;
}

.action-help-button,
.command-help {
  border: 1px solid color-mix(in srgb, var(--border) 82%, transparent);
  background: color-mix(in srgb, var(--surface) 92%, transparent);
  color: var(--muted);
  border-radius: 8px;
  width: 30px;
  height: 30px;
  display: inline-grid;
  place-items: center;
}

.action-help-button:hover,
.command-help:hover {
  color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 62%, var(--border));
}
```

- [ ] **Step 6: Run row tests**

Run:

```bash
cd /home/jmagar/workspace/axon/apps/palette-tauri
pnpm vitest run src/components/palette/ActionList.test.tsx
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
cd /home/jmagar/workspace/axon
git add apps/palette-tauri/src/components/palette/ActionList.tsx apps/palette-tauri/src/components/palette/ActionList.test.tsx apps/palette-tauri/src/App.tsx apps/palette-tauri/src/styles.css
git commit -m "feat(palette): add safe help affordances"
```

---

### Task 6: End-to-End Verification

**Files:**
- Modify only files touched by prior tasks if verification exposes a mismatch.

- [ ] **Step 1: Run focused palette tests**

Run:

```bash
cd /home/jmagar/workspace/axon/apps/palette-tauri
pnpm vitest run src/lib/actionHelp.test.ts src/lib/paletteView.test.ts src/lib/axonClient.test.ts src/lib/useActionRunner.test.tsx src/components/palette/ActionList.test.tsx src/components/palette/OperationResultView.test.tsx
```

Expected: PASS.

- [ ] **Step 2: Run all palette tests**

Run:

```bash
cd /home/jmagar/workspace/axon/apps/palette-tauri
pnpm test
```

Expected: PASS.

- [ ] **Step 3: Run typecheck and build**

Run:

```bash
cd /home/jmagar/workspace/axon/apps/palette-tauri
pnpm typecheck
pnpm vite:build
```

Expected: both PASS.

- [ ] **Step 4: Manual smoke checklist**

Run the palette and verify:

```text
help
help scrape
scrape help
fetch help
crawl --help
?
selected-row ?
```

Expected:

- Each route opens local help.
- No Axon REST request is sent for any help route.
- `?` does not run the selected action.
- Help output uses the remaining result space without introducing nested cards inside assistant bubbles.
- Long route/option text wraps instead of truncating.

- [ ] **Step 5: Commit any verification fixes**

If a fix was needed:

```bash
cd /home/jmagar/workspace/axon
git add apps/palette-tauri/src/lib/actions.ts apps/palette-tauri/src/lib/actionMeta.ts apps/palette-tauri/src/lib/actionHelp.ts apps/palette-tauri/src/lib/axonClient.ts apps/palette-tauri/src/lib/paletteView.ts apps/palette-tauri/src/lib/useActionRunner.ts apps/palette-tauri/src/components/palette/ActionList.tsx apps/palette-tauri/src/components/palette/HelpResultView.tsx apps/palette-tauri/src/components/palette/OperationResultView.tsx apps/palette-tauri/src/App.tsx apps/palette-tauri/src/styles.css apps/palette-tauri/src/lib/actionHelp.test.ts apps/palette-tauri/src/lib/paletteView.test.ts apps/palette-tauri/src/lib/axonClient.test.ts apps/palette-tauri/src/lib/useActionRunner.test.tsx apps/palette-tauri/src/components/palette/ActionList.test.tsx apps/palette-tauri/src/components/palette/OperationResultView.test.tsx
git commit -m "fix(palette): stabilize help verification"
```

If no fix was needed, do not create an empty commit.

---

## Self-Review

Spec coverage:

- `help` as a palette action: Task 1 and Task 2.
- `scrape help` / `crawl help`: Task 2 parser tests and implementation.
- Selected-action `?`: Task 5 row and command-bar affordances.
- Exact command/option visibility: Task 2 metadata plus Task 4 structured renderer.
- No backend route for help: Task 3 no-REST tests and defensive request rejection.

Engineering review coverage:

- `actionMeta.ts` prevents the `actionHelp.ts` / `paletteView.ts` circular import.
- Help is handled before backend/config guards.
- Local actions cannot be sent through `buildActionRequest()`.
- `ActionList` avoids nested buttons before adding `?`.
- No-REST tests cover typed and clicked help entry points.
- Future-option taxonomy was removed; options are plain human-readable strings.

Plan hygiene:

- Banned placeholder patterns were scanned and removed.
- Each test step includes concrete test code.
- Each code-edit task names exact files and exact verification commands.

Dirty-worktree hygiene:

- Each commit command lists exact files.
- Do not run `git add apps/palette-tauri/src`.
- Existing palette polish changes outside these exact file sets must be preserved and not reverted.

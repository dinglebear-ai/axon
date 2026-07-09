import { describe, expect, it } from "vitest";

import { ACTION_REGISTRY, actionBehavior, maybeActionBehavior, type StructuredViewKey } from "./actionRegistry";
import { ACTIONS, type PaletteSubcommand } from "./actions";
import { outputKindFor } from "./format";
import { actionRouteTemplate } from "./axonClient";

// The full set of subcommands the palette can dispatch: every ACTIONS entry,
// which already includes the 24 generated job-lifecycle members.
const ALL_SUBCOMMANDS = ACTIONS.map((a) => a.subcommand);

describe("ACTION_REGISTRY exhaustiveness", () => {
  it("has a behavior entry for every palette subcommand", () => {
    for (const subcommand of ALL_SUBCOMMANDS) {
      expect(ACTION_REGISTRY[subcommand], subcommand).toBeDefined();
    }
  });

  it("covers all 24 job-lifecycle members", () => {
    const lifecycle = ALL_SUBCOMMANDS.filter((s) =>
      /^(crawl|embed|extract|ingest)-(list|status|cancel|cleanup|clear|recover)$/.test(s),
    );
    expect(lifecycle).toHaveLength(24);
    for (const subcommand of lifecycle) {
      expect(ACTION_REGISTRY[subcommand].structuredView, subcommand).toBe("job-lifecycle");
    }
  });

  it("every entry carries a complete behavior shape", () => {
    for (const subcommand of ALL_SUBCOMMANDS) {
      const behavior = ACTION_REGISTRY[subcommand];
      expect(typeof behavior.buildBody, subcommand).toBe("function");
      expect(typeof behavior.formatText, subcommand).toBe("function");
      expect(behavior.route.method, subcommand).toMatch(/^(GET|POST|DELETE)$/);
      expect(behavior.route.path, subcommand).toBeTruthy();
      expect(behavior.actionIcon, subcommand).toBeTruthy();
      expect(behavior.outputIcon, subcommand).toBeTruthy();
      expect(behavior.outputKind, subcommand).toMatch(/^(markdown|code)$/);
    }
  });

  it("structuredView keys are a subset of the StructuredViewKey union", () => {
    const known: Record<StructuredViewKey, true> = {
      help: true,
      files: true,
      scrape: true,
      query: true,
      retrieve: true,
      search: true,
      research: true,
      map: true,
      suggest: true,
      sources: true,
      domains: true,
      doctor: true,
      crawl: true,
      embed: true,
      extract: true,
      ingest: true,
      "ingest-sessions-prepared": true,
      github: true,
      endpoints: true,
      brand: true,
      diff: true,
      screenshot: true,
      dedupe: true,
      "watch-list": true,
      "watch-create": true,
      "watch-run": true,
      "job-lifecycle": true,
    };
    for (const subcommand of ALL_SUBCOMMANDS) {
      const key = ACTION_REGISTRY[subcommand].structuredView;
      if (key !== null) expect(known[key], `${subcommand} -> ${key}`).toBe(true);
    }
  });
});

describe("registry-derived shims preserve behavior", () => {
  it("outputKindFor matches the registry outputKind", () => {
    for (const subcommand of ALL_SUBCOMMANDS) {
      expect(outputKindFor(subcommand), subcommand).toBe(ACTION_REGISTRY[subcommand].outputKind);
    }
  });

  it("markdown output kinds match the pre-registry classification", () => {
    const markdownSubcommands: PaletteSubcommand[] = [
      "ask",
      "chat",
      "scrape",
      "summarize",
      "research",
      "suggest",
      "endpoints",
      "brand",
      "diff",
      "screenshot",
    ];
    for (const subcommand of ALL_SUBCOMMANDS) {
      const expected = markdownSubcommands.includes(subcommand) ? "markdown" : "code";
      expect(outputKindFor(subcommand), subcommand).toBe(expected);
    }
  });

  it("actionRouteTemplate matches the registry route template", () => {
    for (const action of ACTIONS) {
      if (action.kind === "local") continue;
      expect(actionRouteTemplate(action.subcommand), action.subcommand).toEqual(
        ACTION_REGISTRY[action.subcommand].route,
      );
    }
  });

  it("exposes the documented job-lifecycle route templates", () => {
    expect(ACTION_REGISTRY["crawl-status"].route).toEqual({ method: "GET", path: "/v1/crawl/{id}" });
    expect(ACTION_REGISTRY["embed-cancel"].route).toEqual({ method: "POST", path: "/v1/embed/{id}/cancel" });
    expect(ACTION_REGISTRY["ingest-clear"].route).toEqual({ method: "DELETE", path: "/v1/ingest" });
    expect(ACTION_REGISTRY["extract-recover"].route).toEqual({ method: "POST", path: "/v1/extract/recover" });
    expect(ACTION_REGISTRY["watch-run"].route).toEqual({ method: "POST", path: "/v1/watch/{id}/run" });
  });
});

describe("actionBehavior boundaries", () => {
  it("throws on unknown subcommands instead of silently inventing generic behavior", () => {
    expect(() => actionBehavior("not-a-real-subcommand")).toThrow("Unknown palette action: not-a-real-subcommand");
    expect(maybeActionBehavior("not-a-real-subcommand")).toBeNull();
  });
});

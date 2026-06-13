import { describe, expect, it } from "vitest";

import type { HistoryItem } from "@/components/palette/HistoryPanel";
import { ACTIONS } from "@/lib/actions";
import { buildHelpRun } from "@/lib/actionHelp";
import { runStateFromHistory } from "@/lib/historyRun";

function action(subcommand: string) {
  const found = ACTIONS.find((candidate) => candidate.subcommand === subcommand);
  if (!found) throw new Error(`missing action ${subcommand}`);
  return found;
}

describe("runStateFromHistory", () => {
  it("replays local help with its original structured payload", () => {
    const helpRun = buildHelpRun(action("scrape"));
    const item: HistoryItem = {
      action: action("help"),
      target: "scrape",
      status: helpRun.result.status,
      title: helpRun.title,
      subtitle: helpRun.subtitle,
      text: helpRun.text,
      outputKind: helpRun.outputKind,
      result: helpRun.result,
      when: "just now",
    };

    const replay = runStateFromHistory(item);

    expect(replay).toMatchObject({
      kind: "success",
      title: "Scrape URL help",
      subtitle: "scrape help",
      outputKind: "markdown",
      result: {
        path: "palette://help",
        method: "GET",
        payload: { target: { subcommand: "scrape" } },
      },
    });
  });

  it("falls back safely for legacy local help rows without payloads", () => {
    const item: HistoryItem = {
      action: action("help"),
      target: "catalog",
      status: 200,
      title: "Palette help",
      subtitle: "help",
      text: "# Axon Palette Help",
      outputKind: "markdown",
      when: "earlier",
    };

    const replay = runStateFromHistory(item);

    expect(replay).toMatchObject({
      kind: "success",
      title: "Palette help",
      subtitle: "help",
      result: {
        path: "palette://help",
        method: "GET",
        payload: null,
      },
    });
  });

  it("preserves remote error metadata instead of reconstructing it", () => {
    const item: HistoryItem = {
      action: action("scrape"),
      target: "https://example.invalid",
      status: 502,
      title: "Scrape URL failed",
      subtitle: "POST /v1/scrape | HTTP 502",
      text: "bad gateway",
      outputKind: "code",
      result: {
        ok: false,
        status: 502,
        method: "POST",
        path: "/v1/scrape",
        payload: { error: "bad gateway" },
      },
      when: "earlier",
    };

    const replay = runStateFromHistory(item);

    expect(replay).toMatchObject({
      kind: "error",
      title: "Scrape URL failed",
      subtitle: "POST /v1/scrape | HTTP 502",
      result: {
        status: 502,
        path: "/v1/scrape",
        payload: { error: "bad gateway" },
      },
    });
  });
});

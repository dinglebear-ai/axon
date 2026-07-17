import { describe, expect, it } from "vitest";

import {
  actionConfirmationArmed,
  actionConfirmationMessage,
  actionGuard,
  actionNeedsConfirmation,
} from "./actionGuard";
import { ACTIONS } from "./actions";

function action(subcommand: string) {
  const found = ACTIONS.find((candidate) => candidate.subcommand === subcommand);
  if (!found) throw new Error(`missing action ${subcommand}`);
  return found;
}

describe("action safety guards", () => {
  it("marks destructive/stateful commands that must not run from an accidental first Enter", () => {
    for (const subcommand of ["jobs-clear", "jobs-cleanup", "jobs-cancel", "watch-create", "watch-run"]) {
      expect(actionNeedsConfirmation(action(subcommand)), subcommand).toBe(true);
      expect(actionGuard(action(subcommand))?.label, subcommand).toMatch(/confirm|review/i);
    }
  });

  it("does not guard read-only discovery actions", () => {
    for (const subcommand of ["status", "stats", "sources", "domains", "query", "search"]) {
      expect(actionNeedsConfirmation(action(subcommand)), subcommand).toBe(false);
      expect(actionGuard(action(subcommand)), subcommand).toBeNull();
    }
  });

  it("arms confirmation for the exact action and argument only", () => {
    const jobsClear = action("jobs-clear");
    const watchRun = action("watch-run");
    const pending = { subcommand: "watch-run", argument: "00000000-0000-4000-8000-000000000000" };

    expect(actionConfirmationArmed(pending, watchRun, "00000000-0000-4000-8000-000000000000")).toBe(true);
    expect(actionConfirmationArmed(pending, watchRun, "11111111-1111-4111-8111-111111111111")).toBe(false);
    expect(actionConfirmationArmed(pending, jobsClear, "")).toBe(false);
  });

  it("uses different copy for the review step and the armed step", () => {
    const jobsClear = action("jobs-clear");

    expect(actionConfirmationMessage(jobsClear, false)).toMatch(/review/i);
    expect(actionConfirmationMessage(jobsClear, true)).toMatch(/press enter again/i);
  });
});

import { describe, expect, it } from "vitest";

import { ACTIONS } from "./actions";

describe("local interactive actions", () => {
  it("does not auto-run Terminal during keyboard type-ahead or action switching", () => {
    const terminal = ACTIONS.find((action) => action.subcommand === "terminal");
    expect(terminal?.autoRunOnSwitch).not.toBe(true);
  });
});

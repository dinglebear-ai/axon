import { describe, expect, it } from "vitest";
import type { ExecutionLoadoutDraft } from "../../clients/labbyClient";
import {
  bufferFrom,
  catalogCapability,
  changedFields,
  MAX_LOADOUT_MEMBERS,
  reapplyBuffer,
  validateBuffer,
} from "./model";

const draft: ExecutionLoadoutDraft = {
  id: "dev",
  name: "Dev",
  description: null,
  members: [],
  draftRevision: 3,
  desiredActiveRevision: 2,
  effectiveRuntimeRevision: 2,
  restartRequired: false,
};
describe("ExecutionLoadout editor model", () => {
  it("supports all eight families while enforcing server bounds", () => {
    const buffer = bufferFrom("p", draft);
    buffer.members = Array.from({ length: MAX_LOADOUT_MEMBERS + 1 }, (_, i) => ({
      provider: "p",
      family: (
        ["tool", "prompt", "resource", "skill", "agent", "mcp_app", "mcp_server", "plugin"] as const
      )[i % 8],
      memberId: `id-${i}`,
      expectedRevision: "r",
    }));
    expect(validateBuffer(buffer)).toContain("512");
  });
  it("maps only authoritative live tool identities and preserves opaque revision", () => {
    expect(
      catalogCapability({
        kind: "mcpTool",
        id: "mcp:x::y",
        label: "Y",
        description: "",
        source: "x",
        destructive: false,
        contractHash: "r7",
      }),
    ).toEqual({ provider: "x", family: "tool", memberId: "mcp:x::y", expectedRevision: "r7" });
  });
  it("reloads a stale base then deterministically reapplies local edits", () => {
    const local = { ...bufferFrom("p", draft), name: "Local" };
    const current = { ...draft, name: "Remote", draftRevision: 4 };
    expect(changedFields(local, current)).toEqual(["name"]);
    expect(reapplyBuffer(local, current)).toMatchObject({ name: "Local", base: current });
  });
});

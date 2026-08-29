import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BackendProfile } from "../backendProfiles/model";
import { LabbyClient, type LabbyToolDescriptor } from "./labbyClient";

const profile: BackendProfile = {
  id: "labby-1",
  label: "Labby",
  product: "labby",
  origin: "https://labby.example",
  credentialHandle: "labby-token",
  pinnedServerId: null,
  acceptedApiMajor: 1,
};
const descriptor: LabbyToolDescriptor = {
  kind: "mcpTool",
  id: "mcp:github::search",
  label: "Search",
  description: "",
  source: "github",
  destructive: false,
  contractHash: "hash-1",
  contractVersion: 1,
  catalogRevision: "rev-1",
  inputSchema: { type: "object" },
  outputSchema: null,
  annotations: {},
};

describe("LabbyClient exact calls", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    Object.defineProperty(globalThis, "crypto", {
      value: { randomUUID: () => "01234567-89ab-cdef-0123-456789abcdef" },
      configurable: true,
    });
  });
  it("uses only the typed Labby palette transport and proves backend no-LLM evidence", async () => {
    const invoke = vi.spyOn(await import("../invoke"), "invoke").mockResolvedValue({
      ok: true,
      status: 200,
      profileId: profile.id,
      product: "labby",
      requestId: "r",
      payload: {
        id: descriptor.id,
        result: { content: [{ type: "text", text: "<script>inert</script>" }] },
        receipt: {
          requestId: "r",
          auditId: "audit-1",
          toolId: descriptor.id,
          contractHash: "hash-1",
          catalogRevision: "rev-1",
          executionMode: "exact",
          llmInvocations: 0,
          truncated: false,
        },
      },
    });
    const result = await new LabbyClient(profile).execute(descriptor, { q: "rust" }, false);
    expect(result.receipt.llmInvocations).toBe(0);
    expect(invoke).toHaveBeenCalledOnce();
    expect(JSON.stringify(invoke.mock.calls)).not.toMatch(/axon|codex|llm[^I]/i);
  });
  it("rejects receipts that do not confirm exact no-LLM execution", async () => {
    vi.spyOn(await import("../invoke"), "invoke").mockResolvedValue({
      ok: true,
      status: 200,
      profileId: profile.id,
      product: "labby",
      requestId: "r",
      payload: {
        id: descriptor.id,
        result: {},
        receipt: { executionMode: "exact", llmInvocations: 1 },
      },
    });
    await expect(new LabbyClient(profile).execute(descriptor, {}, false)).rejects.toThrow("no-LLM");
  });
});

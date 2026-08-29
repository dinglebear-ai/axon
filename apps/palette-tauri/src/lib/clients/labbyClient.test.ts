import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BackendProfile } from "../backendProfiles/model";
import { LabbyClient, type ArtifactPreview, type LabbyToolDescriptor } from "./labbyClient";

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
  it("requests an actor-authenticated approval for the exact execution context and proposal", async () => {
    const invoke = vi.spyOn(await import("../invoke"), "invoke").mockResolvedValue({
      ok: true,
      status: 200,
      profileId: profile.id,
      product: "labby",
      requestId: "r",
      payload: {
        approvalToken: "single-use",
        expiresAtUnixMs: Date.now() + 30_000,
        executionContextId: "ctx-1",
        toolCallId: "call-1",
      },
    });
    await new LabbyClient(profile).requestApproval({
      executionContextId: "ctx-1",
      turnId: "turn-1",
      proposal: {
        toolCallId: "call-1",
        toolId: "mcp:host::delete",
        contractHash: "sha256:x",
        arguments: { id: 7 },
      },
    });
    expect(invoke).toHaveBeenCalledWith("backend_http_request", {
      request: expect.objectContaining({
        profileId: "labby-1",
        product: "labby",
        method: "POST",
        path: "/v1/palette/agent/approvals",
        body: expect.objectContaining({ executionContextId: "ctx-1", turnId: "turn-1" }),
      }),
    });
  });
  it("fails closed when the authenticated live catalog does not expose admin snippet actions", async () => {
    const invoke = vi.spyOn(await import("../invoke"), "invoke").mockResolvedValue({
      ok: true,
      status: 200,
      profileId: profile.id,
      product: "labby",
      requestId: "r",
      payload: { fingerprint: "reader-catalog", entries: [] },
    });
    await expect(new LabbyClient(profile).listSnippets()).rejects.toThrow("does not expose");
    expect(invoke).toHaveBeenCalledOnce();
  });
  it("binds snippet actions to the live descriptor contract before exact execution", async () => {
    const action = {
      ...descriptor,
      kind: "labbyAction" as const,
      id: "labby:snippets::snippets.validate",
    };
    const invoke = vi
      .spyOn(await import("../invoke"), "invoke")
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        profileId: profile.id,
        product: "labby",
        requestId: "search",
        payload: { fingerprint: "f", entries: [action] },
      })
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        profileId: profile.id,
        product: "labby",
        requestId: "describe",
        payload: action,
      })
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        profileId: profile.id,
        product: "labby",
        requestId: "execute",
        payload: {
          id: action.id,
          result: { valid: true, name: "demo", mode: "body" },
          receipt: {
            requestId: "execute",
            auditId: "audit",
            toolId: action.id,
            contractHash: action.contractHash,
            catalogRevision: action.catalogRevision,
            executionMode: "exact",
            llmInvocations: 0,
            truncated: false,
          },
        },
      });
    const result = await new LabbyClient(profile).validateSnippet(
      "demo",
      "async () => ({ok:true})",
    );
    expect(result.value.valid).toBe(true);
    expect(invoke.mock.calls[2]?.[1]).toMatchObject({
      request: { body: { expectedContractHash: "hash-1" } },
    });
  });
  it("binds artifact lifecycle calls to the authorized Skills service descriptor", async () => {
    const action = {
      ...descriptor,
      kind: "labbyAction" as const,
      id: "labby:skills::agent_library.preview",
    };
    const invoke = vi
      .spyOn(await import("../invoke"), "invoke")
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        profileId: profile.id,
        product: "labby",
        requestId: "search",
        payload: { fingerprint: "f", entries: [action] },
      })
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        profileId: profile.id,
        product: "labby",
        requestId: "describe",
        payload: action,
      })
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        profileId: profile.id,
        product: "labby",
        requestId: "execute",
        payload: {
          id: action.id,
          result: {
            artifact_id: "agent-demo",
            revision_id: "sha256:x",
            render_mode: "inert_text",
            files: [],
          },
          receipt: {
            requestId: "execute",
            auditId: "audit",
            toolId: action.id,
            contractHash: action.contractHash,
            catalogRevision: action.catalogRevision,
            executionMode: "exact",
            llmInvocations: 0,
            truncated: false,
          },
        },
      });
    const result = await new LabbyClient(profile).artifactAction<ArtifactPreview>(
      "agent",
      "preview",
      {
        name: "demo",
        files: [],
      },
    );
    expect(result.value.render_mode).toBe("inert_text");
    expect(invoke.mock.calls[2]?.[1]).toMatchObject({
      request: { body: { id: action.id, expectedContractHash: "hash-1" } },
    });
  });
});

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BackendProfile } from "@/lib/backendProfiles/model";
import { invoke } from "@/lib/invoke";
import { GatewayClient, StaleGatewayError } from "./client";
import { emptyGatewayDraft, type GatewayView, gatewayFingerprint } from "./model";

vi.mock("@/lib/invoke", () => ({ invoke: vi.fn() }));

const profile: BackendProfile = {
  id: "labby-prod",
  label: "Labby",
  product: "labby",
  origin: "https://labby.test",
  credentialHandle: "labby-prod",
  pinnedServerId: "labby_abcdefghijklmnop",
  acceptedApiMajor: 1,
};
const view: GatewayView = {
  revision: "sha256:fixture",
  config: {
    name: "docs",
    enabled: true,
    url: "https://example.test/mcp",
    args: [],
    oauth_enabled: false,
    proxy_resources: false,
    proxy_prompts: false,
  },
  runtime: {
    name: "docs",
    connected: true,
    tool_count: 1,
    resource_count: 0,
    prompt_count: 0,
  },
};

describe("GatewayClient", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    Object.defineProperty(globalThis, "crypto", {
      value: { randomUUID: () => "01234567-89ab-cdef-0123-456789abcdef" },
      configurable: true,
    });
  });

  it("routes draft tests and creates through the selected profile's live gateway route", async () => {
    vi.mocked(invoke).mockResolvedValue({ ok: true, status: 200, payload: view });
    const client = new GatewayClient(profile);
    const draft = { ...emptyGatewayDraft(), name: "docs", url: "https://example.test/mcp" };
    await client.testDraft(draft);
    await client.create(draft);
    expect(
      vi
        .mocked(invoke)
        .mock.calls.map(([, args]) => (args as { request: { path: string } }).request),
    ).toEqual([
      expect.objectContaining({
        profileId: "labby-prod",
        product: "labby",
        method: "POST",
        path: "/v1/gateway",
        body: { action: "gateway.test", params: expect.any(Object) },
      }),
      expect.objectContaining({
        path: "/v1/gateway",
        body: { action: "gateway.add", params: expect.any(Object) },
      }),
    ]);
  });

  it("hydrates only bounded custom rows through typed detail actions", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        payload: [
          { id: "docs", source: "custom_gateway" },
          { id: "lab", source: "in_process" },
        ],
      })
      .mockResolvedValueOnce({ ok: true, status: 200, payload: view });
    await expect(new GatewayClient(profile).list()).resolves.toEqual([view]);
    expect(
      vi
        .mocked(invoke)
        .mock.calls.map(
          ([, args]) => (args as { request: { body: { action: string } } }).request.body.action,
        ),
    ).toEqual(["gateway.list", "gateway.get"]);
  });

  it("fails closed before a stale update", async () => {
    vi.mocked(invoke).mockResolvedValue({
      ok: true,
      status: 200,
      payload: { ...view, revision: "sha256:changed" },
    });
    await expect(
      new GatewayClient(profile).update("docs", emptyGatewayDraft(), gatewayFingerprint(view)),
    ).rejects.toBeInstanceOf(StaleGatewayError);
    expect(invoke).toHaveBeenCalledOnce();
  });
});

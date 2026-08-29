import { beforeEach, describe, expect, it, vi } from "vitest";
import { approveCodexOperation, CODEX_MUTATIONS, prepareCodexOperation, readCodexSnapshot } from "./codexControl";
import type { Client } from "./axonClient";

const invoke = vi.fn();
vi.mock("./invoke", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const client: Client = { baseUrl: "https://axon.example", headers: { Authorization: "Bearer redacted" } };

beforeEach(() => invoke.mockReset());

describe("Codex control client", () => {
  it("reads the server-host snapshot through the shared bridge", async () => {
    invoke.mockResolvedValue({ ok: true, status: 200, payload: { status: { state: "ready" } } });
    await expect(readCodexSnapshot(client)).resolves.toMatchObject({ status: { state: "ready" } });
    expect(invoke.mock.calls[0][1].request.path).toBe("/v1/codex");
  });

  it("uses prepare and separate approval requests", async () => {
    invoke
      .mockResolvedValueOnce({ ok: true, status: 200, payload: { id: 7, phase: "pending", request_digest: "abc" } })
      .mockResolvedValueOnce({ ok: true, status: 200, payload: { approval_capability: "single-use" } });
    const operation = await prepareCodexOperation(client, { ...CODEX_MUTATIONS.pluginInstall, params: { plugin: "demo" } });
    expect(operation.id).toBe(7);
    await expect(approveCodexOperation(client, 7)).resolves.toBe("single-use");
    expect(invoke.mock.calls[1][1].request.path).toContain("/approve");
  });
});

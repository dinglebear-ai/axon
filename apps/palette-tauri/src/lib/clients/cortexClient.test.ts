import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BackendProfile } from "../backendProfiles/model";
import { invoke } from "../invoke";
import { CortexClient } from "./cortexClient";

vi.mock("../invoke", () => ({ invoke: vi.fn() }));
const profile: BackendProfile = {
  id: "cortex-prod",
  label: "Cortex",
  product: "cortex",
  origin: "https://cortex.test",
  credentialHandle: "cortex-prod",
  pinnedServerId: "cortex_abcdefghijklmnop",
  acceptedApiMajor: 1,
};
describe("CortexClient", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue({ ok: true, status: 200, payload: { logs: [] } });
  });
  it("encodes filters and keeps auth in the selected profile", async () => {
    await new CortexClient(profile).logs({
      query: "host:a & error",
      cursor: "opaque+/=",
      limit: 100,
    });
    expect(invoke).toHaveBeenCalledWith("backend_http_request", {
      request: expect.objectContaining({
        profileId: "cortex-prod",
        product: "cortex",
        method: "GET",
        path: "/api/search?query=host%3Aa+%26+error&cursor=opaque%2B%2F%3D&limit=100",
      }),
    });
  });
  it("sends the required typed entity selector for graph lookup", async () => {
    await new CortexClient(profile).graph({ entityType: "service_instance", key: "nashost/plex" });
    expect(invoke).toHaveBeenCalledWith("backend_http_request", {
      request: expect.objectContaining({
        path: "/api/graph/around?entity_type=service_instance&key=nashost%2Fplex&depth=1&limit=100&evidence_sample_limit=3&payload_budget=32768",
      }),
    });
  });
  it("propagates cancellation to Tauri", async () => {
    let release!: () => void;
    vi.mocked(invoke).mockImplementation((command) =>
      command === "backend_http_request"
        ? new Promise((resolve) => {
            release = () => resolve({ ok: true, status: 200, payload: { logs: [] } });
          })
        : Promise.resolve(undefined),
    );
    const abort = new AbortController();
    const pending = new CortexClient(profile).logs({}, abort.signal);
    abort.abort();
    expect(invoke).toHaveBeenCalledWith(
      "backend_cancel_request",
      expect.objectContaining({ requestId: expect.any(String) }),
    );
    release();
    await pending;
  });
  it("uses a committed cursor with the complete rendered-session identity", async () => {
    await new CortexClient(profile).renderedSession(
      { project: "axon", tool: "codex", sessionId: "session/1", host: "dev host" },
      "cortex-session-v1:42",
    );
    expect(invoke).toHaveBeenCalledWith("backend_http_request", {
      request: expect.objectContaining({
        path: "/api/sessions/rendered?project=axon&tool=codex&session_id=session%2F1&host=dev+host&cursor=cortex-session-v1%3A42&limit=200",
      }),
    });
  });
});

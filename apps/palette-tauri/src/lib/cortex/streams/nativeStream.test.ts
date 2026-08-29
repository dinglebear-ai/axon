import { describe, expect, it, vi } from "vitest";
import type { BackendProfile } from "../../backendProfiles/model";
import { followCortexStream } from "./nativeStream";

const profile: BackendProfile = {
  id: "cortex-a",
  label: "Cortex A",
  product: "cortex",
  origin: "https://cortex.example",
  credentialHandle: "cred-a",
  credentialGeneration: "g1",
  pinnedServerId: "server-a",
  acceptedApiMajor: 1,
};

function response(id: string) {
  const bytes = new TextEncoder().encode(`id: ${id}\nevent: log\ndata: {"id":"${id}"}\n\n`);
  return new Response(
    new ReadableStream({
      start(controller) {
        controller.enqueue(bytes);
        controller.close();
      },
    }),
  );
}

describe("followCortexStream", () => {
  it("reconnects with only the last committed cursor and stops on cancellation", async () => {
    const fetch = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(response("cursor-1"))
      .mockResolvedValueOnce(response("cursor-2"));
    const controller = new AbortController();
    const ids: string[] = [];
    await followCortexStream(
      profile,
      "logs",
      {},
      3,
      (event) => {
        if (event.id) ids.push(event.id);
        if (event.id === "cursor-2") controller.abort();
      },
      controller.signal,
    );
    expect(ids).toEqual(["cursor-1", "cursor-2"]);
    expect(fetch.mock.calls[1]?.[0].toString()).toContain("cursor=cursor-1");
  });

  it("reconnects retryable thrown stream errors from the last committed cursor", async () => {
    const controller = new AbortController();
    const fetch = vi.spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(response("cursor-1"))
      .mockRejectedValueOnce(new TypeError("connection reset"))
      .mockImplementationOnce(async (input) => {
        expect(input.toString()).toContain("cursor=cursor-1");
        controller.abort();
        return response("cursor-2");
      });
    fetch.mockClear();
    await followCortexStream(profile, "logs", {}, 4, () => undefined, controller.signal);
    expect(fetch).toHaveBeenCalledTimes(3);
  });
});

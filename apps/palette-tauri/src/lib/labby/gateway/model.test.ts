import { describe, expect, it } from "vitest";
import {
  boundedGatewayRows,
  emptyGatewayDraft,
  gatewayChallenge,
  isPrivateTarget,
  MAX_GATEWAY_ROWS,
} from "./model";

describe("gateway model", () => {
  it.each([
    "http://localhost:3000",
    "http://127.0.0.1",
    "https://10.0.0.2",
    "http://172.20.1.2",
    "http://192.168.1.2",
  ])("discloses private target %s", (url) => {
    expect(isPrivateTarget(url)).toBe(true);
  });

  it("discloses stdio and OAuth behavior before server-side execution", () => {
    const challenge = gatewayChallenge({
      ...emptyGatewayDraft(),
      transport: "stdio",
      command: "npx",
      oauthEnabled: true,
    });
    expect(challenge).toMatchObject({ stdio: true, oauth: true });
    expect(challenge.messages).toHaveLength(2);
  });

  it("bounds custom gateway rows and rejects malformed responses", () => {
    expect(
      boundedGatewayRows(
        Array.from({ length: 150 }, (_, index) => ({
          id: String(index),
          source: "custom_gateway",
        })),
      ),
    ).toHaveLength(MAX_GATEWAY_ROWS);
    expect(boundedGatewayRows([{ id: "built-in", source: "in_process" }])).toEqual([]);
    expect(() => boundedGatewayRows({})).toThrow(/invalid gateway list/i);
  });
});

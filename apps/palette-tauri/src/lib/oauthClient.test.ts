import { describe, expect, it } from "vitest";
import { describeOauthStatus, type OauthStatus } from "./oauthClient";

const signedOut: OauthStatus = { signedIn: false, scope: null, expiresAtUnix: null, serverUrl: null };

describe("describeOauthStatus", () => {
  it("reports signed-out state", () => {
    const result = describeOauthStatus(signedOut);
    expect(result.tone).toBe("neutral");
    expect(result.label).toBe("Not signed in");
  });

  it("reports an active session", () => {
    const status: OauthStatus = {
      signedIn: true,
      scope: "axon:read axon:write",
      expiresAtUnix: 4_102_444_800,
      serverUrl: "https://axon.example.com",
    };
    const result = describeOauthStatus(status, 1_700_000_000);
    expect(result.tone).toBe("success");
    expect(result.label).toBe("Signed in");
    expect(result.detail).toContain("axon.example.com");
  });

  it("flags an expired session", () => {
    const status: OauthStatus = {
      signedIn: true,
      scope: "axon:read",
      expiresAtUnix: 1_000,
      serverUrl: "https://axon.example.com",
    };
    const result = describeOauthStatus(status, 2_000);
    expect(result.tone).toBe("error");
    expect(result.label).toBe("Session expired");
  });

  it("flags credentials issued for a different server", () => {
    const status: OauthStatus = {
      signedIn: false,
      scope: null,
      expiresAtUnix: null,
      serverUrl: "https://other.example.com",
    };
    const result = describeOauthStatus(status);
    expect(result.tone).toBe("error");
    expect(result.label).toBe("Different server");
    expect(result.detail).toContain("other.example.com");
  });
});

import { describe, expect, it } from "vitest";
import {
  activeProfile,
  assertCompatibleIdentity,
  type BackendProfile,
  type ProductIdentity,
} from "./model";

const profile: BackendProfile = {
  id: "axon-default",
  label: "Axon",
  product: "axon",
  origin: "https://axon.example",
  credentialHandle: null,
  pinnedServerId: "axon_1234567890abcdef",
  acceptedApiMajor: 1,
};
const identity: ProductIdentity = {
  contract_version: "1.0.0",
  product: "axon",
  server_id: "axon_1234567890abcdef",
  product_version: "7.2.2",
  api_version: { major: 1, minor: 0 },
  capabilities: ["query"],
  auth: { modes: ["static_bearer"], credential_generation: "1" },
  streams: { transport: "sse", resume: "none" },
};

describe("assertCompatibleIdentity", () => {
  it("accepts the pinned product identity", () =>
    expect(assertCompatibleIdentity(profile, identity)).toBe(identity));
  it("rejects wrong products", () =>
    expect(() => assertCompatibleIdentity(profile, { ...identity, product: "labby" })).toThrow(
      /product profile/,
    ));
  it("rejects incompatible majors", () =>
    expect(() =>
      assertCompatibleIdentity(profile, { ...identity, api_version: { major: 2, minor: 0 } }),
    ).toThrow(/Unsupported/));
  it("rejects identity swaps", () =>
    expect(() =>
      assertCompatibleIdentity(profile, { ...identity, server_id: "axon_abcdefghijklmnop" }),
    ).toThrow(/re-trust/));
});

describe("activeProfile", () => {
  it("never falls back to the first profile", () => {
    const second = { ...profile, id: "second", label: "Second" };
    expect(activeProfile([profile, second], {}, "axon")).toBeNull();
    expect(activeProfile([profile, second], { axon: "second" }, "axon")).toEqual(second);
    expect(activeProfile([profile], { labby: profile.id }, "labby")).toBeNull();
  });
});

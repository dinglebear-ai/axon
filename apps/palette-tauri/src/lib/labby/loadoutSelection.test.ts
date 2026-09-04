import { beforeEach, describe, expect, it } from "vitest";
import type { BackendProfile } from "@/lib/backendProfiles/model";
import {
  clearLoadoutSelection,
  loadoutRequestFields,
  writeDelegation,
  writeLoadoutSelection,
} from "./loadoutSelection";

const profile: BackendProfile = {
  id: "labby-a",
  label: "A",
  product: "labby",
  origin: "https://labby.test",
  credentialHandle: "cred",
  pinnedServerId: "server-a",
  acceptedApiMajor: 1,
};

describe("loadout selection", () => {
  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
  });
  it("preserves no-loadout request behavior", () =>
    expect(loadoutRequestFields(profile)).toEqual({}));
  it("sends only an opaque revision-bound context selection", () => {
    writeLoadoutSelection(profile, {
      integrationId: "server-a",
      loadoutId: "review",
      name: "Review",
      expectedRevision: 7,
      mode: "context",
    });
    expect(loadoutRequestFields(profile)).toMatchObject({
      loadout: { integrationId: "server-a", loadoutId: "review", expectedRevision: 7 },
    });
    expect(JSON.stringify(loadoutRequestFields(profile))).not.toContain("members");
  });
  it("fails closed without delegation and isolates session credentials by profile", () => {
    writeLoadoutSelection(profile, {
      integrationId: "server-a",
      loadoutId: "agent",
      name: "Agent",
      expectedRevision: 4,
      mode: "agent",
    });
    expect(() => loadoutRequestFields(profile)).toThrow("audience-bound");
    writeDelegation(profile.id, "delegated");
    expect(loadoutRequestFields(profile)).toMatchObject({
      agent: { delegationToken: "delegated" },
    });
    clearLoadoutSelection(profile);
    expect(loadoutRequestFields(profile)).toEqual({});
  });
});

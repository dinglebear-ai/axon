import { describe, expect, it } from "vitest";
import type { ProductIdentity } from "../backendProfiles/model";
import {
  boundedAppend,
  boundedByItemsAndBytes,
  capabilityAvailable,
  CORTEX_RETAINED_BYTES,
  visibleWindow,
} from "./viewModel";

const identity: ProductIdentity = {
  contract_version: "1.0.0",
  product: "cortex",
  server_id: "cortex_abcdefghijklmnop",
  product_version: "1",
  api_version: { major: 1, minor: 0 },
  capabilities: ["logs.query"],
  auth: { modes: ["bearer"], credential_generation: "1" },
  streams: { transport: "none", resume: "none" },
};
describe("Cortex view model", () => {
  it("fails closed when a capability is absent", () => {
    expect(capabilityAvailable(identity, "logs")).toBe(true);
    expect(capabilityAvailable(identity, "fleet")).toBe(false);
    expect(capabilityAvailable(null, "logs")).toBe(false);
  });
  it("caps retained rows and virtualizes large result sets", () => {
    expect(
      boundedAppend(
        Array.from({ length: 490 }, (_, i) => i),
        Array.from({ length: 40 }, (_, i) => 490 + i),
      ),
    ).toHaveLength(500);
    const view = visibleWindow(
      Array.from({ length: 1000 }, (_, i) => i),
      26000,
    );
    expect(view.rows.length).toBeLessThanOrEqual(80);
    expect(view.start).toBeGreaterThan(0);
    expect(view.top + view.bottom).toBeGreaterThan(0);
  });
  it("caps retained tail bytes while preserving the newest event", () => {
    expect(
      boundedByItemsAndBytes(
        [],
        ["a".repeat(CORTEX_RETAINED_BYTES), "latest"],
        (item) => item.length,
      ),
    ).toEqual(["latest"]);
  });
});

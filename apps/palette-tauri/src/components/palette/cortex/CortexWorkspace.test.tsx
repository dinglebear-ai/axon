// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BackendProfile, ProductIdentity } from "@/lib/backendProfiles/model";
import { invoke } from "@/lib/invoke";
import { CortexWorkspace } from "./CortexWorkspace";

vi.mock("@/lib/invoke", () => ({ invoke: vi.fn() }));

const profile: BackendProfile = {
  id: "cortex-prod",
  label: "Production Cortex",
  product: "cortex",
  origin: "https://cortex.test",
  credentialHandle: "cortex-prod",
  pinnedServerId: "cortex_abcdefghijklmnop",
  acceptedApiMajor: 1,
};
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

describe("CortexWorkspace", () => {
  afterEach(cleanup);
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command !== "backend_http_request") return undefined;
      const path = (args as { request: { path: string } }).request.path;
      if (path === "/v1/integration/identity") return { ok: true, status: 200, payload: identity };
      return {
        ok: true,
        status: 200,
        payload: {
          logs: [
            {
              id: 7,
              timestamp: "2026-08-28T20:00:00Z",
              hostname: "host-a",
              severity: "warning",
              message: '<img src=x onerror="alert(1)"> hostile markdown **stays text**',
              parse_warnings: ["partial parser result"],
            },
          ],
        },
      };
    });
  });

  it("verifies identity, renders hostile logs as text, and gates absent capabilities", async () => {
    const { container } = render(<CortexWorkspace profile={profile} />);
    await screen.findByText("cortex_abcdefghijklmnop");
    const form = screen.getByLabelText("Filter or correlation anchor").closest("form");
    expect(form).not.toBeNull();
    if (!form) throw new Error("query form missing");
    fireEvent.submit(form);
    expect(await screen.findByText(/<img src=x onerror=/)).toBeInTheDocument();
    expect(container.querySelector("img")).toBeNull();
    expect(screen.getByText("Parse warning: partial parser result")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "fleet" }));
    expect(await screen.findByText("fleet unavailable")).toBeInTheDocument();
    await waitFor(() =>
      expect(
        vi
          .mocked(invoke)
          .mock.calls.filter(
            ([command, args]) =>
              command === "backend_http_request" &&
              (args as { request: { path: string } }).request.path === "/api/fleet-state",
          ),
      ).toHaveLength(0),
    );
  });
});

// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BackendProfile } from "@/lib/backendProfiles/model";
import { GatewayWorkspace } from "./GatewayWorkspace";

const gateway = vi.hoisted(() => ({
  list: vi.fn(),
  get: vi.fn(),
  testDraft: vi.fn(),
  create: vi.fn(),
  update: vi.fn(),
  remove: vi.fn(),
  reload: vi.fn(),
}));
vi.mock("@/lib/labby/gateway/client", () => ({
  GatewayClient: class {
    list = gateway.list;
    get = gateway.get;
    testDraft = gateway.testDraft;
    create = gateway.create;
    update = gateway.update;
    remove = gateway.remove;
    reload = gateway.reload;
  },
  GatewayClientError: class extends Error {
    status = 0;
  },
}));

const profile: BackendProfile = {
  id: "labby-prod",
  label: "Labby",
  product: "labby",
  origin: "https://labby.test",
  credentialHandle: "labby-prod",
  pinnedServerId: "labby_abcdefghijklmnop",
  acceptedApiMajor: 1,
};

describe("GatewayWorkspace", () => {
  afterEach(cleanup);
  beforeEach(() => {
    for (const mock of Object.values(gateway)) mock.mockReset();
    gateway.list.mockResolvedValue([]);
  });

  it("loads the selected profile's upstreams and requires confirmation before stdio testing", async () => {
    render(<GatewayWorkspace profile={profile} />);
    await screen.findByText(/0 upstreams/i);
    fireEvent.click(screen.getByRole("button", { name: /add upstream/i }));
    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "local" } });
    fireEvent.change(screen.getByLabelText("Transport"), { target: { value: "stdio" } });
    fireEvent.change(screen.getByLabelText("Command"), { target: { value: "npx" } });
    expect(screen.getByText(/may start the configured command/i)).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Test" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent(/confirm test/i);
    expect(gateway.testDraft).not.toHaveBeenCalled();
  });

  it("renders a bounded unavailable-runtime error", async () => {
    gateway.list.mockRejectedValue(new Error("offline"));
    render(<GatewayWorkspace profile={profile} />);
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("offline"));
  });
});

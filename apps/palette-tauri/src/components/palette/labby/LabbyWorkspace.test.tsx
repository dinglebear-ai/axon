// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { BackendProfile } from "@/lib/backendProfiles/model";
import { LabbyWorkspace } from "./LabbyWorkspace";

vi.mock("./ArtifactWorkspace", () => ({ ArtifactWorkspace: () => <div>Artifacts workspace</div> }));
vi.mock("./LabbyExactToolRunner", () => ({
  LabbyExactToolRunner: () => <div>Exact tools workspace</div>,
}));
vi.mock("./LabbySnippetWorkspace", () => ({
  LabbySnippetWorkspace: () => <div>Snippets workspace</div>,
}));
vi.mock("./LoadoutWorkspace", () => ({ LoadoutWorkspace: () => <div>Loadouts workspace</div> }));

const profile: BackendProfile = {
  id: "labby-prod",
  label: "Labby",
  product: "labby",
  origin: "https://labby.test",
  credentialHandle: "labby-prod",
  pinnedServerId: "labby_abcdefghijklmnop",
  acceptedApiMajor: 1,
};

describe("LabbyWorkspace", () => {
  afterEach(cleanup);

  it("exposes Gateway as a first-class workspace tab", () => {
    render(<LabbyWorkspace profile={profile} />);
    fireEvent.click(screen.getByRole("button", { name: "Gateway" }));
    expect(screen.getByRole("region", { name: "Labby gateway administration" })).toBeVisible();
  });
});
